//! The call dock: a strip pinned to the bottom of the sidebar for as long as
//! you are in a call.
//!
//! A call outlives the channel you started it in — you can read somewhere else
//! while it runs — so its controls cannot live in the conversation header, the
//! way they used to. They live here, where they are reachable from every
//! channel, and the dock doubles as the way back to the call's own channel.
//!
//! It is the bottom bar of the sidebar's [`adw::ToolbarView`], which is the
//! idiomatic slot for this: space is reserved rather than overlaid, the
//! separator and background come free, and it survives navigation because it
//! sits outside the content stack.

use adw::prelude::*;

use crate::avatars::Avatars;
use crate::state::SharedState;

pub struct CallDock {
    pub widget: gtk::Box,
    avatar: adw::Avatar,
    title: gtk::Label,
    subtitle: gtk::Label,
    mute: gtk::Button,
    screen: gtk::Button,
    camera: gtk::Button,
    record: gtk::Button,
}

impl CallDock {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        on_open: impl Fn() + 'static,
        on_mute: impl Fn() + 'static,
        on_screen: impl Fn() + 'static,
        on_camera: impl Fn() + 'static,
        on_record: impl Fn() + 'static,
        on_leave: impl Fn() + 'static,
    ) -> Self {
        let avatar = adw::Avatar::builder().size(28).build();

        let title = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        title.add_css_class("heading");
        let subtitle = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        subtitle.add_css_class("caption");
        subtitle.add_css_class("dim-label");

        let labels = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        labels.append(&title);
        labels.append(&subtitle);

        let summary_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .build();
        summary_box.append(&avatar);
        summary_box.append(&labels);
        summary_box.append(&gtk::Image::from_icon_name("go-next-symbolic"));

        // The whole summary is the way back to the call's channel — a chevron
        // alone would be a very small target for the most likely click here.
        let summary = gtk::Button::builder()
            .child(&summary_box)
            .tooltip_text("Go to the call")
            .build();
        summary.add_css_class("flat");
        summary.add_css_class("call-dock-summary");
        summary.connect_clicked(move |_| on_open());

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .build();

        let mute = icon_button("microphone-disabled-symbolic", "Unmute", on_mute);
        let screen = icon_button("video-display-symbolic", "Share your screen", on_screen);
        let camera = icon_button("camera-web-symbolic", "Turn the camera on", on_camera);
        let record = icon_button("media-record-symbolic", "Record the call", on_record);
        for button in [&mute, &screen, &camera, &record] {
            controls.append(button);
        }

        let leave = icon_button("call-stop-symbolic", "Leave the call", on_leave);
        leave.add_css_class("destructive-action");
        leave.set_halign(gtk::Align::End);
        leave.set_hexpand(true);
        controls.append(&leave);

        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        widget.add_css_class("call-dock");
        widget.append(&summary);
        widget.append(&controls);

        CallDock {
            widget,
            avatar,
            title,
            subtitle,
            mute,
            screen,
            camera,
            record,
        }
    }

    /// Fills the dock in from the call we are in. Returns whether there is one
    /// — the caller reveals or hides the bar on that.
    pub fn refresh(&self, state: &SharedState, avatars: &Avatars) -> bool {
        let st = state.borrow();
        let Some(call) = st.call.as_ref() else {
            return false;
        };

        // Server-side voice activity only ever names other people, so a quiet
        // call falls back to the channel — never to "you are talking".
        let speaker = call.speaking.first();
        match speaker.and_then(|id| st.users.get(id)) {
            Some(user) => {
                let name = user.display_name(st.teammate_name_display());
                avatars.apply(&self.avatar, &user.id, &name);
                self.title.set_text(&format!("{name} is talking"));
            }
            None => {
                self.avatar.set_show_initials(false);
                self.avatar.set_custom_image(None::<&gtk::gdk::Paintable>);
                self.avatar.set_icon_name(Some("audio-headphones-symbolic"));
                self.title.set_text("In a call");
            }
        }

        let channel = st
            .channel(&call.channel_id)
            .map(|c| st.channel_title(c))
            .unwrap_or_else(|| "a channel".to_string());
        let people = st.active_calls.get(&call.channel_id).map_or(1, Vec::len);
        let sharing = if call.sharing.is_empty() {
            String::new()
        } else {
            " · sharing".to_string()
        };
        self.subtitle
            .set_text(&format!("{channel} · {people} in the call{sharing}"));

        set_state(
            &self.mute,
            !call.muted,
            if call.muted {
                ("microphone-disabled-symbolic", "Unmute")
            } else {
                ("audio-input-microphone-symbolic", "Mute")
            },
            "suggested-action",
        );
        set_state(
            &self.screen,
            call.screen.is_some(),
            (
                "video-display-symbolic",
                if call.screen.is_some() {
                    "Stop sharing your screen"
                } else {
                    "Share your screen"
                },
            ),
            "suggested-action",
        );
        set_state(
            &self.camera,
            call.camera.is_some(),
            (
                "camera-web-symbolic",
                if call.camera.is_some() {
                    "Turn the camera off"
                } else {
                    "Turn the camera on"
                },
            ),
            "suggested-action",
        );
        set_state(
            &self.record,
            call.recording,
            (
                "media-record-symbolic",
                if call.recording {
                    "Stop recording"
                } else {
                    "Record the call"
                },
            ),
            "destructive-action",
        );
        true
    }
}

fn icon_button(icon: &str, tooltip: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.connect_clicked(move |_| on_click());
    button
}

fn set_state(button: &gtk::Button, on: bool, look: (&str, &str), class: &str) {
    button.set_icon_name(look.0);
    button.set_tooltip_text(Some(look.1));
    if on {
        button.add_css_class(class);
    } else {
        button.remove_css_class(class);
    }
}
