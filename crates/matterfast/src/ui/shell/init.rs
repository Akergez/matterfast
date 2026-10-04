use gpui_kit::{App, KeyBinding};

use super::actions::{
    ClosePanel, FocusSearch, NewChannel, NextUnread, OpenInbox, OpenSettings, PreviousUnread,
    QuickSwitch, Quit, CONTEXT,
};
use super::matterfast::{current, Matterfast};
use crate::background::Background;
use crate::notifications::Notifier;
use crate::runtime;

/// Sets up everything that outlives a window: the notification service, the
/// shortcuts, and what happens when the last window goes away.
pub fn init(cx: &mut App) {
    let (notifier, responses) = Notifier::start();
    cx.set_global(Matterfast {
        ui: None,
        window: None,
        notifier,
    });

    // Somebody pressed one of our notifications.
    runtime::receive(responses, |response, cx| {
        if let Some(ui) = current(cx) {
            ui.notification_pressed(&response.tag, response.action.as_deref(), cx);
        }
        true
    });

    // The shortcuts a chat client is expected to have. Anything reachable
    // only by mouse is reachable only slowly.
    cx.bind_keys([
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-k", QuickSwitch, Some(CONTEXT)),
        KeyBinding::new("ctrl-f", FocusSearch, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-i", OpenInbox, Some(CONTEXT)),
        KeyBinding::new("escape", ClosePanel, Some(CONTEXT)),
        KeyBinding::new("ctrl-n", NewChannel, Some(CONTEXT)),
        KeyBinding::new("ctrl-,", OpenSettings, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-down", NextUnread, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-up", PreviousUnread, Some(CONTEXT)),
    ]);
    // Closing the window does not quit, so something else has to.
    cx.on_action(|_: &Quit, cx| cx.quit());

    cx.on_window_closed(|cx, _| {
        let Some(app) = cx.try_global::<Matterfast>() else {
            return;
        };
        let (ui, window) = (app.ui.clone(), app.window.clone());
        // Ours, or some other window — a file dialog, say?
        if window.is_some_and(|(handle, _)| cx.windows().contains(&handle)) {
            return;
        }
        cx.global_mut::<Matterfast>().window = None;
        match ui {
            // The session carries on with nothing on screen: the socket stays
            // connected and notifications keep arriving. Presenting it again
            // builds a new window around the same session.
            Some(ui) if Background::enabled() => {
                ui.detach(cx);
                tracing::info!("last window closed; staying in the background");
            }
            _ => cx.quit(),
        }
    })
    .detach();
}
