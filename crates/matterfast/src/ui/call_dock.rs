//! The call dock: a strip pinned to the bottom of the sidebar for as long as
//! you are in a call.
//!
//! A call outlives the channel you started it in — you can read somewhere else
//! while it runs — so its controls cannot live in the conversation header, the
//! way they used to. They live here, where they are reachable from every
//! channel, and the dock doubles as the way back to the call's own channel.
//!
//! It sits under the channel list, outside anything that scrolls or switches,
//! which is why it survives navigation; on a window too narrow to show the
//! sidebar beside the conversation it moves under the conversation instead.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, ElementId, FontWeight};

use super::kit::{self, Lucide};
use super::{Action, Ui};
use crate::state::{ActiveCall, AppState, SharedState};

/// What the host can do to somebody else in the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAction {
    MuteOthers,
    EndCall,
    Mute,
    StopSharing,
    LowerHand,
    Remove,
    MakeHost,
}

/// The reactions offered in a call: how people agree without interrupting.
const CALL_REACTIONS: [(&str, &str); 4] = [
    ("+1", "👍"),
    ("clap", "👏"),
    ("joy", "😂"),
    ("open_mouth", "😮"),
];

pub struct CallDock {
    /// One line of live transcription, or a reaction; empty when there is
    /// neither. A subtitle, not a transcript: it is replaced as it goes.
    caption: RefCell<String>,
}

impl CallDock {
    pub fn new() -> Self {
        CallDock {
            caption: RefCell::new(String::new()),
        }
    }

    /// Shows one line of live transcription, or clears it.
    pub fn set_caption(&self, who: &str, text: &str, cx: &mut App) {
        *self.caption.borrow_mut() = caption_line(who, text);
        cx.refresh_windows();
    }

    /// The dock is drawn from the call in the state, so a refresh is a frame.
    /// Answers whether there is a call to show at all.
    pub fn refresh(&self, state: &SharedState, cx: &mut App) -> bool {
        cx.refresh_windows();
        let in_call = state.borrow().call.is_some();
        if !in_call {
            // A caption belongs to the call it was said in.
            self.caption.borrow_mut().clear();
        }
        in_call
    }
}

/// A caption as it is shown. A reaction arrives with no words around it, so
/// it reads better as "Anna 👏" than as "Anna: 👏" — the caller passes the
/// colon when it wants one.
fn caption_line(who: &str, text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let separator = if who.is_empty() { "" } else { " " };
    format!("{who}{separator}{text}")
}

/// The dock's two lines: who is talking, and where.
///
/// Server-side voice activity only ever names other people, so a quiet call
/// falls back to the channel — never to "you are talking". The first value is
/// the speaker's id and name, when there is one.
fn summary(call: &ActiveCall, st: &AppState) -> (Option<(String, String)>, String, String) {
    let speaker = call
        .speaking
        .first()
        .and_then(|id| st.users.get(id))
        .map(|user| {
            (
                user.id.clone(),
                user.display_name(st.teammate_name_display()),
            )
        });
    let title = match &speaker {
        Some((_, name)) => format!("{name} is talking"),
        None => "In a call".to_string(),
    };
    let channel = st
        .channel(&call.channel_id)
        .map(|c| st.channel_title(c))
        .unwrap_or_else(|| "a channel".to_string());
    let people = st.active_calls.get(&call.channel_id).map_or(1, Vec::len);
    let sharing = if call.sharing.is_empty() {
        ""
    } else {
        " · sharing"
    };
    (
        speaker,
        title,
        format!("{channel} · {people} in the call{sharing}"),
    )
}

/// The host controls that apply to one participant. Only those: offering
/// "stop sharing" to someone who is not sharing is a button that does
/// nothing.
fn host_controls(sharing: bool, hand_up: bool) -> Vec<(HostAction, &'static str)> {
    let mut controls = vec![(HostAction::Mute, "Mute them")];
    if sharing {
        controls.push((HostAction::StopSharing, "Stop their screen share"));
    }
    if hand_up {
        controls.push((HostAction::LowerHand, "Lower their hand"));
    }
    controls.push((HostAction::MakeHost, "Make them the host"));
    controls.push((HostAction::Remove, "Remove from call"));
    controls
}

fn host_icon(action: HostAction) -> Lucide {
    match action {
        HostAction::Mute | HostAction::MuteOthers => Lucide::MicOff,
        HostAction::StopSharing => Lucide::MonitorOff,
        HostAction::LowerHand => Lucide::Hand,
        HostAction::MakeHost => Lucide::Crown,
        HostAction::Remove => Lucide::UserMinus,
        HostAction::EndCall => Lucide::X,
    }
}

/// A dock button that shows whether its feature is on.
fn toggle(
    id: &'static str,
    icon: Lucide,
    tooltip: &'static str,
    on: bool,
    danger: bool,
) -> Button {
    Button::new(id)
        .icon(Icon::from(icon))
        .small()
        .tooltip(tooltip)
        .when(on && danger, |button| button.danger())
        .when(on && !danger, |button| button.primary())
        .when(!on, |button| button.ghost())
}

/// The participant list: face, name, and whatever is true of them right now —
/// talking, hand up, muted.
fn roster(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let st = ui.state.borrow();
    let Some(call) = st.call.as_ref() else {
        return div().into_any_element();
    };
    let theme = cx.theme();
    let i_am_host = call.host_id == st.me.id;
    let people = st
        .active_calls
        .get(&call.channel_id)
        .cloned()
        .unwrap_or_default();

    let mut list = v_flex().w(px(280.)).gap_0p5();
    for user_id in &people {
        let name = st
            .users
            .get(user_id)
            .map(|u| u.display_name(st.teammate_name_display()))
            .unwrap_or_else(|| "Someone".to_string());
        let mut row = h_flex()
            .id(ElementId::Name(format!("participant-{user_id}").into()))
            .gap_2()
            .h(px(32.))
            .items_center()
            .child(kit::avatar(ui, user_id, &name, 24.))
            .child(div().flex_1().min_w_0().truncate().child(name));

        // A raised hand is a request and outranks the rest.
        if let Some(place) = call.hands.iter().position(|id| id == user_id) {
            row = row.child(kit::with_tooltip(
                "hand",
                format!("✋{}", place + 1),
                "Wants to speak",
            ));
        }
        if call.speaking.first() == Some(user_id) {
            row = row.child(kit::with_tooltip(
                "talking",
                div().text_color(theme.success).child(Lucide::Mic),
                "Talking",
            ));
        } else if call.muted_users.contains(user_id) {
            row = row.child(kit::with_tooltip(
                "muted",
                div()
                    .text_color(theme.muted_foreground)
                    .child(Lucide::MicOff),
                "Muted",
            ));
        }

        // Host controls, on the people they apply to. Shown only to the host,
        // and never against the host's own row: muting yourself is the button
        // already in the dock.
        if i_am_host && user_id != &st.me.id {
            if let Some(session_id) = call.sessions.get(user_id) {
                let controls = host_controls(
                    call.sharing.contains(user_id),
                    call.hands.iter().any(|id| id == user_id),
                );
                for (index, (action, tooltip)) in controls.into_iter().enumerate() {
                    // Most host routes address a session; making someone host
                    // addresses the person, since every session of theirs
                    // gains it at once.
                    let target = match action {
                        HostAction::MakeHost => user_id.clone(),
                        _ => session_id.clone(),
                    };
                    row = row.child(
                        kit::icon_button(("host", index), host_icon(action), tooltip)
                            .xsmall()
                            .on_click(ui.click(move |ui, cx| {
                                ui.dispatch(Action::HostControl(target.clone(), action), cx)
                            })),
                    );
                }
            }
        }

        if call.sharing.contains(user_id) {
            row = row.child(kit::with_tooltip(
                "sharing",
                div().text_color(theme.primary).child(Lucide::Monitor),
                "Sharing a screen",
            ));
        }
        list = list.child(row);
    }
    div()
        .id("roster")
        .max_h(px(280.))
        .overflow_y_scroll()
        .child(list)
        .into_any_element()
}

/// Draws the dock, or nothing when we are not in a call.
pub fn render(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let st = ui.state.borrow();
    let call = st.call.as_ref()?;
    let theme = cx.theme();
    let (speaker, title, subtitle) = summary(call, &st);
    let i_am_host = call.host_id == st.me.id;
    let muted = call.muted;
    let sharing = call.screen.is_some();
    let camera = call.camera.is_some();
    let recording = call.recording;
    // Your own hand: the button reflects whether it is up, since you cannot
    // see yourself in the roster otherwise.
    let my_hand = call.hands.iter().any(|id| id == &st.me.id);
    drop(st);

    let face: AnyElement = match &speaker {
        Some((id, name)) => kit::avatar(ui, id, name, 28.).into_any_element(),
        None => div()
            .size(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme.muted)
            .child(Lucide::Headphones)
            .into_any_element(),
    };

    // The whole summary is the way back to the call's channel — a chevron
    // alone would be a very small target for the most likely click here.
    let summary = h_flex()
        .id("call-summary")
        .flex_1()
        .min_w_0()
        .gap_2p5()
        .p_1()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(theme.list_hover))
        .child(face)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(subtitle),
                ),
        )
        .child(Lucide::ChevronRight)
        .on_click(ui.click(|ui, cx| ui.dispatch(Action::OpenCallChannel, cx)));

    // Who is actually in there, with their hands and microphones. A count
    // answers "is it worth joining"; the list answers "who is that".
    let people = Popover::new("call-people")
        .trigger(kit::icon_button(
            "people",
            Lucide::Users,
            "Who is in the call",
        ))
        .content({
            let ui = ui.clone();
            move |_, _, cx| roster(&ui, cx)
        });

    let react = Popover::new("call-react")
        .trigger(kit::icon_button("react", Lucide::FaceSlightlySmiling, "React"))
        .content({
            let ui = ui.clone();
            move |_, _, _| {
                let mut row = h_flex().gap_0p5();
                for (index, (name, glyph)) in CALL_REACTIONS.into_iter().enumerate() {
                    row = row.child(
                        Button::new(("call-reaction", index))
                            .ghost()
                            .small()
                            .label(glyph)
                            .tooltip(format!(":{name}:"))
                            .on_click(ui.click(move |ui, cx| {
                                ui.dispatch(
                                    Action::CallReaction(name.to_string(), glyph.to_string()),
                                    cx,
                                )
                            })),
                    );
                }
                row
            }
        });

    let mut controls = h_flex()
        .gap_0p5()
        .items_center()
        .child(react)
        .child(
            toggle(
                "mute",
                if muted { Lucide::MicOff } else { Lucide::Mic },
                if muted { "Unmute" } else { "Mute" },
                !muted,
                false,
            )
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleMute, cx))),
        )
        .child(
            toggle(
                "screen",
                Lucide::Monitor,
                if sharing {
                    "Stop sharing your screen"
                } else {
                    "Share your screen"
                },
                sharing,
                false,
            )
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleScreen, cx))),
        )
        .child(
            toggle(
                "camera",
                Lucide::Video,
                if camera {
                    "Turn the camera off"
                } else {
                    "Turn the camera on"
                },
                camera,
                false,
            )
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleCamera, cx))),
        )
        .child(
            toggle(
                "hand",
                Lucide::Hand,
                if my_hand {
                    "Lower your hand"
                } else {
                    "Raise your hand"
                },
                my_hand,
                false,
            )
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleHand, cx))),
        )
        .child(
            toggle(
                "record",
                Lucide::Disc,
                if recording {
                    "Stop recording"
                } else {
                    "Record the call"
                },
                recording,
                true,
            )
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleRecording, cx))),
        );

    // Host-only, and destructive for everyone else in the call, so they sit
    // apart from the controls that only affect you.
    if i_am_host {
        controls = controls
            .child(
                kit::icon_button("mute-others", Lucide::MicOff, "Mute everyone else").on_click(
                    ui.click(|ui, cx| {
                        // These take no target: they apply to the whole call.
                        ui.dispatch(
                            Action::HostControl(String::new(), HostAction::MuteOthers),
                            cx,
                        )
                    }),
                ),
            )
            .child(
                Button::new("end-call")
                    .icon(Icon::from(Lucide::X))
                    .small()
                    .danger()
                    .tooltip("End the call for everyone")
                    .on_click(ui.click(|ui, cx| {
                        ui.dispatch(Action::HostControl(String::new(), HostAction::EndCall), cx)
                    })),
            );
    }

    controls = controls.child(div().flex_1()).child(
        Button::new("leave")
            .icon(Icon::from(Lucide::PhoneOff))
            .small()
            .danger()
            .tooltip("Leave the call")
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleCall, cx))),
    );

    let caption = ui.dock.caption.borrow().clone();
    Some(
        v_flex()
            .id("call-dock")
            .flex_none()
            .w_full()
            .gap_1()
            .p_1p5()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(h_flex().gap_0p5().items_center().child(summary).child(people))
            // It takes no space when the server is not captioning.
            .when(!caption.is_empty(), |dock| {
                dock.child(
                    div()
                        .px_1()
                        .text_xs()
                        .line_clamp(2)
                        .text_color(theme.muted_foreground)
                        .child(caption),
                )
            })
            .child(controls)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caption_names_its_speaker_and_an_empty_one_clears_the_line() {
        assert_eq!(caption_line("Anna:", "hello there"), "Anna: hello there");
        assert_eq!(caption_line("Anna", "👏"), "Anna 👏");
        assert_eq!(caption_line("", "hello"), "hello");
        assert_eq!(caption_line("Anna:", "   "), "");
        assert_eq!(caption_line("", ""), "");
    }

    #[test]
    fn the_host_is_only_offered_what_applies() {
        let actions = |sharing, hand| -> Vec<HostAction> {
            host_controls(sharing, hand)
                .into_iter()
                .map(|(action, _)| action)
                .collect()
        };
        assert_eq!(
            actions(false, false),
            [HostAction::Mute, HostAction::MakeHost, HostAction::Remove]
        );
        assert!(actions(true, false).contains(&HostAction::StopSharing));
        assert!(actions(false, true).contains(&HostAction::LowerHand));
        // Removing someone is always last: the one that cannot be undone
        // should not sit where a different button was a moment ago.
        assert_eq!(actions(true, true).last(), Some(&HostAction::Remove));
    }

    #[test]
    fn every_host_action_has_an_icon_of_its_own_kind() {
        // Muting one and muting all are the same act on a different number of
        // people, and nothing else may borrow the microphone.
        for action in [
            HostAction::StopSharing,
            HostAction::LowerHand,
            HostAction::MakeHost,
            HostAction::Remove,
            HostAction::EndCall,
        ] {
            assert_ne!(
                host_icon(action).path(),
                host_icon(HostAction::Mute).path(),
                "{action:?}"
            );
        }
    }
}
