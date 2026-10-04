use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, AnyElement, App, ObjectFit};
use mattermost_api::models::FileInfo;

use super::clock::clock;
use super::constants::{POSTER_HEIGHT, POSTER_WIDTH};
use super::formats::is_audio;
use super::stage::Stage;
use crate::ui::kit::{self, Lucide};
use crate::ui::Ui;

/// Draws the player for one attached file.
pub fn player(ui: &Rc<Ui>, index: usize, file: &FileInfo, cx: &App) -> AnyElement {
    let player = ui.media_player(&file.id);
    let stage = player.stage.get();
    let theme = cx.theme();
    let label = format!("{}  ·  {}", file.name, file.human_size());
    let progress = player
        .playback
        .borrow()
        .as_ref()
        .and_then(|playback| playback.progress())
        .map(|(position, duration)| clock(position, duration));

    // Built where it is used: the same button sits over the poster before
    // playing and in the controls under the picture after.
    let play = || {
        let play_file = file.clone();
        Button::new("play")
            .icon(match stage {
                Stage::Playing => Lucide::Pause,
                Stage::Ended => Lucide::RotateCcw,
                _ => Lucide::Play,
            })
            .small()
            .tooltip(match stage {
                Stage::Playing => "Pause",
                Stage::Ended => "Play again",
                _ => "Play",
            })
            .on_click(ui.click(move |ui, cx| ui.play_media(&play_file, cx)))
    };

    let error = player.error.borrow().clone();
    let note = error.map(|message| {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(message)
    });

    // Audio has nothing to show: it gets controls and no poster.
    if is_audio(file) {
        return v_flex()
            .id(("audio", index))
            .mt_1()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_color(theme.muted_foreground)
                    .child(if stage == Stage::Loading {
                        Spinner::new().small().into_any_element()
                    } else {
                        play().ghost().into_any_element()
                    })
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when_some(progress, |row, progress| {
                        row.child(div().flex_none().text_xs().child(progress))
                    }),
            )
            .when_some(note, |column, note| column.child(note))
            .into_any_element();
    }

    let picture = match stage {
        Stage::Playing | Stage::Paused | Stage::Ended => player.frame.borrow().clone(),
        _ => None,
    }
    .or_else(|| ui.poster(file));

    let mut face = div()
        .relative()
        .w(px(POSTER_WIDTH))
        .max_w_full()
        .h(px(POSTER_HEIGHT))
        .rounded_md()
        .overflow_hidden()
        .bg(gpui_kit::black());
    if let Some(picture) = picture {
        face = face.child(img(picture).size_full().object_fit(ObjectFit::Contain));
    }
    // The button sits over the picture until it plays; after that the
    // controls under it are how it is paused.
    if matches!(stage, Stage::Idle | Stage::Loading | Stage::Ended) {
        face = face.child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(if stage == Stage::Loading {
                    Spinner::new().large().into_any_element()
                } else {
                    play().primary().into_any_element()
                }),
        );
    }

    let mut controls = h_flex()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(theme.muted_foreground);
    if matches!(stage, Stage::Playing | Stage::Paused) {
        let expand_id = file.id.clone();
        controls = controls.child(play().ghost()).child(
            kit::icon_button("expand", Lucide::Maximize, "Fill the window").on_click(ui.click(
                move |ui, cx| {
                    ui.media_player(&expand_id).expanded.set(true);
                    cx.refresh_windows();
                },
            )),
        );
    }
    controls = controls
        .child(div().flex_1().min_w_0().truncate().child(label))
        .when_some(progress, |row, progress| {
            row.child(div().flex_none().child(progress))
        });

    v_flex()
        .id(("video", index))
        .mt_1()
        .gap_1()
        .w(px(POSTER_WIDTH))
        .max_w_full()
        .child(face)
        .child(controls)
        .when_some(note, |column, note| column.child(note))
        .into_any_element()
}
