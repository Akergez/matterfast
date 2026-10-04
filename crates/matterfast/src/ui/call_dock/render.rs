use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, FontWeight};

use super::constants::CALL_REACTIONS;
use super::host_action::HostAction;
use super::roster::roster;
use super::summary::summary;
use super::toggle::toggle;
use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, Ui};

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
