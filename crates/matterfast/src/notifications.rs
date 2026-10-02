//! Desktop notifications, over the freedesktop notification service.
//!
//! Spoken to directly rather than through the toolkit, because a chat client
//! needs two things of a notification that the toolkit's own does not do on
//! Linux: a second message in the same conversation has to *replace* the
//! first rather than stack on top of it, and an incoming-call notification
//! has to be taken back down when the call is answered somewhere else.
//!
//! Both are in the protocol — `Notify` takes the id of the notification to
//! replace, and `CloseNotification` exists — so this keeps the map from our
//! own tags to the ids the service hands back, and uses it.
//!
//! Nothing here touches the window: it runs on Tokio, takes commands down one
//! channel and reports what the person pressed up another.

use std::collections::HashMap;

use futures_util::StreamExt;
use zbus::zvariant::Value;

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// The action key the service reports when the notification's body is
/// activated rather than one of its buttons.
const DEFAULT_ACTION: &str = "default";

/// One notification to raise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notice {
    /// Our own identity for it. Raising another with the same tag replaces
    /// this one, and the tag is what comes back when it is pressed.
    pub tag: String,
    pub title: String,
    pub body: String,
    /// Worth interrupting someone for: a call, not a message.
    pub urgent: bool,
    /// Buttons, as (id, label). The id comes back in [`Response::action`].
    pub actions: Vec<(String, String)>,
}

/// The person did something with a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub tag: String,
    /// The button pressed, or `None` for the notification itself.
    pub action: Option<String>,
}

enum Command {
    Show(Notice),
    Withdraw(String),
}

/// A handle on the notification task. Cheap to clone; dropping the last one
/// stops the task.
#[derive(Clone)]
pub struct Notifier {
    tx: tokio::sync::mpsc::UnboundedSender<Command>,
}

impl Notifier {
    /// Starts the task and returns the stream of responses.
    pub fn start() -> (Notifier, async_channel::Receiver<Response>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (responses_tx, responses_rx) = async_channel::unbounded();
        crate::runtime::runtime().spawn(run(rx, responses_tx));
        (Notifier { tx }, responses_rx)
    }

    pub fn show(&self, notice: Notice) {
        let _ = self.tx.send(Command::Show(notice));
    }

    /// Takes a notification back down, if it is still up.
    pub fn withdraw(&self, tag: &str) {
        let _ = self.tx.send(Command::Withdraw(tag.to_string()));
    }
}

/// The flat `[key, label, key, label, …]` list the service takes. The body
/// itself is offered as an action too, which is what makes clicking it report
/// anything at all.
fn action_list(notice: &Notice) -> Vec<&str> {
    let mut list = vec![DEFAULT_ACTION, "Open"];
    for (id, label) in &notice.actions {
        list.push(id);
        list.push(label);
    }
    list
}

/// A body may be read as markup by the service, and a message is arbitrary
/// text from another person: `<b>` in it must arrive as those three
/// characters.
fn escape_body(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// What a reported action key means: the body, or one of our buttons.
fn action_from_key(key: &str) -> Option<String> {
    (key != DEFAULT_ACTION).then(|| key.to_string())
}

async fn run(
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
    responses: async_channel::Sender<Response>,
) {
    let proxy = match connect().await {
        Ok(proxy) => proxy,
        Err(error) => {
            // No session bus, or no notification service on it. The
            // application is still usable; it just cannot tap anyone on the
            // shoulder. The commands are drained so the senders never block.
            tracing::warn!(%error, "desktop notifications are unavailable");
            while commands.recv().await.is_some() {}
            return;
        }
    };
    let (mut invoked, mut closed) = match (
        proxy.receive_signal("ActionInvoked").await,
        proxy.receive_signal("NotificationClosed").await,
    ) {
        (Ok(invoked), Ok(closed)) => (invoked, closed),
        (Err(error), _) | (_, Err(error)) => {
            tracing::warn!(%error, "could not listen for notification responses");
            while commands.recv().await.is_some() {}
            return;
        }
    };

    // Tag → the id the service gave it, for everything still on screen.
    let mut shown: HashMap<String, u32> = HashMap::new();

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Show(notice)) => {
                    let replaces = shown.get(&notice.tag).copied().unwrap_or(0);
                    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
                    hints.insert("desktop-entry", Value::from(crate::APP_ID));
                    hints.insert("urgency", Value::U8(if notice.urgent { 2 } else { 1 }));
                    let reply = proxy
                        .call_method(
                            "Notify",
                            &(
                                "Matterfast",
                                replaces,
                                crate::APP_ID,
                                notice.title.as_str(),
                                escape_body(&notice.body).as_str(),
                                action_list(&notice),
                                hints,
                                -1i32,
                            ),
                        )
                        .await;
                    match reply.and_then(|message| message.body().deserialize::<u32>()) {
                        Ok(id) => {
                            shown.insert(notice.tag, id);
                        }
                        Err(error) => tracing::warn!(%error, "could not raise a notification"),
                    }
                }
                Some(Command::Withdraw(tag)) => {
                    if let Some(id) = shown.remove(&tag) {
                        let _ = proxy.call_method("CloseNotification", &(id,)).await;
                    }
                }
                // Every handle is gone: the application is shutting down.
                None => return,
            },
            Some(message) = invoked.next() => {
                let Ok((id, key)) = message.body().deserialize::<(u32, String)>() else {
                    continue;
                };
                // Another application's notification reports here too; only
                // the ones in the map are ours.
                let Some(tag) = shown
                    .iter()
                    .find(|(_, shown_id)| **shown_id == id)
                    .map(|(tag, _)| tag.clone())
                else {
                    continue;
                };
                let _ = responses
                    .send(Response { tag, action: action_from_key(&key) })
                    .await;
            }
            Some(message) = closed.next() => {
                if let Ok((id, _reason)) = message.body().deserialize::<(u32, u32)>() {
                    shown.retain(|_, shown_id| *shown_id != id);
                }
            }
        }
    }
}

async fn connect() -> zbus::Result<zbus::Proxy<'static>> {
    let connection = zbus::Connection::session().await?;
    zbus::Proxy::new(&connection, SERVICE, PATH, SERVICE).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_an_action_and_buttons_follow_it() {
        let notice = Notice {
            actions: vec![
                ("join".into(), "Join".into()),
                ("dismiss".into(), "Dismiss".into()),
            ],
            ..Default::default()
        };
        assert_eq!(
            action_list(&notice),
            ["default", "Open", "join", "Join", "dismiss", "Dismiss"]
        );
        assert_eq!(action_list(&Notice::default()), ["default", "Open"]);
    }

    #[test]
    fn a_pressed_body_is_not_a_button() {
        assert_eq!(action_from_key("default"), None);
        assert_eq!(action_from_key("join"), Some("join".to_string()));
    }

    #[test]
    fn a_message_cannot_smuggle_markup_into_a_notification() {
        assert_eq!(
            escape_body("<b>hi</b> & <a href=\"x\">y</a>"),
            "&lt;b&gt;hi&lt;/b&gt; &amp; &lt;a href=\"x\"&gt;y&lt;/a&gt;"
        );
    }
}
