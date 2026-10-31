use std::rc::Rc;

use gpui_adaptive::prim::{column, pressable, row, tooltip};
use gpui_adaptive::{ActiveColors as _, ActiveScale as _, Command, Popup, Text};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App};

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
    let colors = cx.colors();
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
            .bg(colors.sunken)
            .child(Lucide::Headphones)
            .into_any_element(),
    };

    // The whole summary is the way back to the call's channel — a chevron
    // alone would be a very small target for the most likely click here.
    let summary = pressable("call-summary", cx)
        .flex_1()
        .min_w_0()
        .gap_2p5()
        .p_1()
        .rounded_md()
        .child(face)
        .child(
            column()
                .flex_1()
                .min_w_0()
                .child(Text::title(title).truncate())
                .child(Text::small(subtitle).muted().truncate()),
        )
        .child(Lucide::ChevronRight)
        .on_click(ui.click(|ui, cx| ui.dispatch(Action::OpenCallChannel, cx)));

    // Who is actually in there, with their hands and microphones. A count
    // answers "is it worth joining"; the list answers "who is that".
    let people = Popup::new("call-people", kit::icon_button("people", Lucide::Users, "Who is in the call"), {
        let ui = ui.clone();
        move |_, _, cx| roster(&ui, cx)
    });

    let react = Popup::new("call-react", kit::icon_button("react", Lucide::FaceSlightlySmiling, "React"), {
        let ui = ui.clone();
        move |close, _, cx| {
            let scale = cx.scale();
            let mut reactions = row().gap_0p5();
            for (index, (name, glyph)) in CALL_REACTIONS.into_iter().enumerate() {
                let close = close.clone();
                let react = ui.run(move |ui, cx| {
                    ui.dispatch(Action::CallReaction(name.to_string(), glyph.to_string()), cx)
                });
                reactions = reactions.child(
                    pressable(("call-reaction", index), cx)
                        .justify_center()
                        .size(scale.target)
                        .rounded(scale.radius_control)
                        .text_size(scale.title)
                        .child(glyph)
                        .tooltip(tooltip(format!(":{name}:")))
                        .on_click(move |_, window, cx| {
                            close(window, cx);
                            react(window, cx);
                        }),
                );
            }
            reactions.into_any_element()
        }
    });

    let mut controls = row()
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
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleMute, cx)))
            .as_icon_button(),
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
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleScreen, cx)))
            .as_icon_button(),
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
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleCamera, cx)))
            .as_icon_button(),
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
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleHand, cx)))
            .as_icon_button(),
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
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleRecording, cx)))
            .as_icon_button(),
        );

    // Host-only, and destructive for everyone else in the call, so they sit
    // apart from the controls that only affect you.
    if i_am_host {
        controls = controls
            .child(
                kit::icon_button("mute-others", Lucide::MicOff, "Mute everyone else").on_trigger(
                    ui.run(|ui, cx| {
                        // These take no target: they apply to the whole call.
                        ui.dispatch(
                            Action::HostControl(String::new(), HostAction::MuteOthers),
                            cx,
                        )
                    }),
                ).as_icon_button(),
            )
            .child(
                Command::new("end-call", "End the call for everyone")
                    .icon(Lucide::X)
                    .destructive()
                    .on_trigger(ui.run(|ui, cx| {
                        ui.dispatch(Action::HostControl(String::new(), HostAction::EndCall), cx)
                    }))
                    .as_icon_button(),
            );
    }

    controls = controls.child(div().flex_1()).child(
        Command::new("leave", "Leave the call")
            .icon(Lucide::PhoneOff)
            .destructive()
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleCall, cx)))
            .as_icon_button(),
    );

    let caption = ui.dock.caption.borrow().clone();
    Some(
        column()
            .id("call-dock")
            .flex_none()
            .w_full()
            .gap_1()
            .p_1p5()
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.surface)
            .text_color(colors.text)
            .child(row().gap_0p5().items_center().child(summary).child(people))
            // It takes no space when the server is not captioning.
            .when(!caption.is_empty(), |dock| {
                dock.child(
                    div()
                        .px_1()
                        .text_xs()
                        .line_clamp(2)
                        .text_color(colors.muted)
                        .child(caption),
                )
            })
            .child(controls)
            .into_any_element(),
    )
}
