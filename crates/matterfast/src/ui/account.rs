//! Editing your own account: the profile and the custom status.
//!
//! Like [`super::dialogs`], nothing here talks to the server or to
//! [`crate::state`] — each function takes callbacks and hands back what the
//! person chose, so the API calls stay in [`super`] with the client.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Days, Local, NaiveTime, TimeZone};
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable, Size, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, FontWeight, RenderImage, SharedString, Window};

use super::dialogs::{self, Field, Form};
use super::Ui;
use crate::runtime;

/// What the other clients offer, with the timeout each one carries: emoji,
/// its shortcode, the wording, and an index into [`DURATIONS`].
const SUGGESTIONS: &[(&str, &str, &str, usize)] = &[
    ("📅", "calendar", "In a meeting", 2),
    ("🍔", "hamburger", "At lunch", 1),
    ("🤒", "face_with_thermometer", "Out sick", 4),
    ("🏠", "house", "Working from home", 4),
    ("🌴", "palm_tree", "On holiday", 5),
];

/// The clear-after choices, in menu order. The index into this table is what
/// [`expires_at`] reads; index 0 never expires.
const DURATIONS: [(&str, &str); 6] = [
    ("0", "Don't clear"),
    ("1", "30 minutes"),
    ("2", "1 hour"),
    ("3", "4 hours"),
    ("4", "Today"),
    ("5", "This week"),
];

/// The profile dialog's body: the picture, a way to change it, and the names.
struct ProfileEditor {
    form: Entity<Form>,
    name: SharedString,
    /// The picture as it stands: the server's, or the one just chosen.
    picture: Option<Arc<RenderImage>>,
    on_avatar: Rc<dyn Fn(PathBuf, &mut App)>,
}

impl ProfileEditor {
    fn choose_picture(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a picture".into()),
        });
        let on_avatar = self.on_avatar.clone();
        cx.spawn(async move |editor, cx| {
            // Cancelled, which is not an error worth showing.
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            // Show it straight away: the upload and the reload of the server's
            // copy both happen much later.
            let preview = path.clone();
            cx.update(|cx| {
                let editor = editor.clone();
                runtime::spawn(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            image::open(&preview)
                                .ok()
                                .map(|picture| crate::avatars::render_image(picture.into_rgba8()))
                        })
                        .await
                        .ok()
                        .flatten()
                    },
                    move |picture, cx| {
                        let _ = editor.update(cx, |editor, cx| {
                            if let Some(picture) = picture {
                                editor.picture = Some(Arc::new(picture));
                                cx.notify();
                            }
                        });
                    },
                );
                on_avatar(path, cx);
            });
        })
        .detach();
    }
}

impl Render for ProfileEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let avatar = Avatar::new()
            .name(self.name.clone())
            .with_size(Size::Size(px(96.)));
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .items_center()
                    .gap_3()
                    .child(match &self.picture {
                        Some(picture) => avatar.src(picture.clone()),
                        None => avatar,
                    })
                    // The picker can be opened again after a mis-click.
                    .child(
                        Button::new("change-picture")
                            .label("Change picture…")
                            .small()
                            .on_click(cx.listener(|editor, _, window, cx| {
                                editor.choose_picture(window, cx)
                            })),
                    ),
            )
            .child(self.form.clone())
    }
}

/// Edit your own profile. `current` is (first_name, last_name, nickname,
/// position). `on_save` gets the same four. `on_avatar` gets a chosen image
/// file.
pub fn edit_profile(
    ui: &Rc<Ui>,
    cx: &mut App,
    current: (String, String, String, String),
    on_save: impl Fn(String, String, String, String, &mut App) + 'static,
    on_avatar: impl Fn(PathBuf, &mut App) + 'static,
) {
    let (first_name, last_name, nickname, position) = current;
    let picture = {
        let me = ui.state.borrow().me.id.clone();
        ui.avatars.texture(&me)
    };
    let on_save = Rc::new(on_save);
    let on_avatar: Rc<dyn Fn(PathBuf, &mut App)> = Rc::new(on_avatar);
    ui.with_window(cx, move |window, cx| {
        let form = cx.new(|cx| {
            Form::new(
                vec![
                    Field::text("First name", &first_name),
                    Field::text("Last name", &last_name),
                    Field::text("Nickname", &nickname),
                    Field::text("Position", &position),
                ],
                window,
                cx,
            )
        });
        let editor = cx.new(|_| ProfileEditor {
            form: form.clone(),
            name: format!("{first_name} {last_name}").trim().to_string().into(),
            picture,
            on_avatar,
        });
        window.open_dialog(cx, move |dialog, _, _| {
            let form = form.clone();
            let on_save = on_save.clone();
            dialog
                .title("Edit profile")
                .child(editor.clone())
                .footer(dialogs::footer("Save"))
                .on_ok(move |_, _, cx| {
                    let values: Vec<String> = form
                        .read(cx)
                        .values(cx)
                        .iter()
                        .map(|value| value.text().trim().to_string())
                        .collect();
                    let on_save = on_save.clone();
                    cx.defer(move |cx| {
                        on_save(
                            values[0].clone(),
                            values[1].clone(),
                            values[2].clone(),
                            values[3].clone(),
                            cx,
                        )
                    });
                    true
                })
        });
    });
}

/// The status dialog's body: the three fields, and things to fill them with.
struct StatusEditor {
    form: Entity<Form>,
    recents: Vec<(String, String)>,
}

impl StatusEditor {
    /// Fills the fields rather than submitting: the wording is often nearly
    /// right, and editing it beats retyping it.
    fn fill(
        &mut self,
        emoji: &str,
        text: &str,
        duration: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form.update(cx, |form, cx| {
            form.set_text(0, emoji, window, cx);
            form.set_text(1, text, window, cx);
            if let Some(duration) = duration {
                form.set_choice(2, duration, cx);
            }
        });
    }
}

impl Render for StatusEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let heading = |text: &'static str| {
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(text)
        };
        let row = |id: (&'static str, usize)| {
            h_flex()
                .id(id)
                .gap_2()
                .px_2()
                .h(px(34.))
                .items_center()
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(theme.list_hover))
        };

        let mut column = v_flex().gap_3().child(self.form.clone());

        if !self.recents.is_empty() {
            let mut recents = v_flex().child(heading("Recent"));
            for (index, (emoji, text)) in self.recents.iter().enumerate() {
                let (fill_emoji, fill_text) = (emoji.clone(), text.clone());
                recents = recents.child(
                    row(("recent", index))
                        .child(crate::emoji::label(emoji))
                        .child(div().flex_1().min_w_0().truncate().child(text.clone()))
                        .on_click(cx.listener(move |editor, _, window, cx| {
                            editor.fill(&fill_emoji, &fill_text, None, window, cx)
                        })),
                );
            }
            column = column.child(recents);
        }

        // The ones people actually pick, with the timeout each one implies —
        // "in a meeting" is an hour, "on holiday" is a week. Typing that out
        // every time is the reason nobody sets a status.
        let mut suggestions = v_flex().child(heading("Suggestions"));
        for (index, (glyph, shortcode, label, duration)) in SUGGESTIONS.iter().enumerate() {
            suggestions = suggestions.child(
                row(("suggestion", index))
                    .child(*glyph)
                    .child(div().flex_1().child(*label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(DURATIONS[*duration].1),
                    )
                    .on_click(cx.listener(move |editor, _, window, cx| {
                        editor.fill(shortcode, label, Some(*duration), window, cx)
                    })),
            );
        }
        column.child(suggestions)
    }
}

/// Set a custom status. `current` is (emoji_shortcode, text).
/// `on_set` gets (emoji, text, expires_at_unix_millis or 0 for never).
/// `on_clear` clears it.
pub fn custom_status(
    ui: &Rc<Ui>,
    cx: &mut App,
    current: (String, String),
    recents: Vec<(String, String)>,
    on_set: impl Fn(String, String, i64, &mut App) + 'static,
    on_clear: impl Fn(&mut App) + 'static,
) {
    let (current_emoji, current_text) = current;
    let is_set = !current_emoji.is_empty() || !current_text.is_empty();
    let on_set = Rc::new(on_set);
    let on_clear = Rc::new(on_clear);
    ui.with_window(cx, move |window, cx| {
        let form = cx.new(|cx| {
            let mut form = Form::new(
                vec![
                    Field::text("Emoji", &current_emoji),
                    Field::text("Message", &current_text),
                    Field::choice("Clear after", &DURATIONS, "0"),
                ],
                window,
                cx,
            );
            form.set_note("The emoji is a shortcode without colons, like coffee.");
            form
        });
        let editor = cx.new(|_| StatusEditor {
            form: form.clone(),
            recents,
        });
        window.open_dialog(cx, move |dialog, _, _| {
            let form = form.clone();
            let on_set = on_set.clone();
            let on_clear = on_clear.clone();
            let mut footer = dialogs::footer("Set status");
            // Nothing to clear until there is a status, so the button is not
            // offered.
            if is_set {
                footer = footer.child(
                    Button::new("clear")
                        .label("Clear status")
                        .danger()
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let on_clear = on_clear.clone();
                            cx.defer(move |cx| on_clear(cx));
                        }),
                );
            }
            dialog
                .title("Set a status")
                .child(editor.clone())
                .footer(footer)
                .on_ok(move |_, _, cx| {
                    let (values, duration) = {
                        let form = form.read(cx);
                        (form.values(cx), form.choice(2))
                    };
                    let emoji = values[0].text().trim().trim_matches(':').to_string();
                    let text = values[1].text().trim().to_string();
                    let expires = expires_at(Local::now(), duration);
                    let on_set = on_set.clone();
                    cx.defer(move |cx| on_set(emoji, text, expires, cx));
                    true
                })
        });
    });
}

/// When the status picked at `base` should expire, in Unix milliseconds.
/// `choice` indexes [`DURATIONS`]; 0 — and anything that cannot be computed —
/// means "never", which is the answer that loses the least.
fn expires_at<Tz: TimeZone>(base: DateTime<Tz>, choice: usize) -> i64 {
    let time = match choice {
        1 => Some(base + chrono::Duration::minutes(30)),
        2 => Some(base + chrono::Duration::hours(1)),
        3 => Some(base + chrono::Duration::hours(4)),
        4 => end_of_day(&base),
        // The week ends on Sunday: 7 - today days from now, with Monday as 1.
        5 => base
            .clone()
            .checked_add_days(Days::new(
                7 - base.weekday().number_from_monday() as u64,
            ))
            .and_then(|day| end_of_day(&day)),
        _ => None,
    };
    time.map_or(0, |time| time.timestamp() * 1000)
}

/// One second before midnight on `day`, in its own local time.
fn end_of_day<Tz: TimeZone>(day: &DateTime<Tz>) -> Option<DateTime<Tz>> {
    day.timezone()
        .from_local_datetime(
            &day.date_naive()
                .and_time(NaiveTime::from_hms_opt(23, 59, 59)?),
        )
        .latest()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn base() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 2, 10, 30, 0).unwrap()
    }

    #[test]
    fn never_is_zero() {
        assert_eq!(expires_at(base(), 0), 0);
        // An index past the table is a bug elsewhere; it must not expire early.
        assert_eq!(expires_at(base(), 99), 0);
    }

    #[test]
    fn offsets_are_added_to_the_base() {
        let start = base().timestamp() * 1000;
        assert_eq!(expires_at(base(), 1) - start, 30 * 60 * 1000);
        assert_eq!(expires_at(base(), 2) - start, 60 * 60 * 1000);
        assert_eq!(expires_at(base(), 3) - start, 4 * 60 * 60 * 1000);
    }

    #[test]
    fn today_ends_tonight() {
        let end = Local
            .timestamp_millis_opt(expires_at(base(), 4))
            .single()
            .unwrap();
        assert_eq!((end.year(), end.month(), end.day()), (2026, 9, 2));
        assert_eq!((end.hour(), end.minute()), (23, 59));
    }

    #[test]
    fn the_week_ends_on_sunday_night() {
        let end = Local
            .timestamp_millis_opt(expires_at(base(), 5))
            .single()
            .unwrap();
        assert_eq!(end.weekday().number_from_monday(), 7);
        assert_eq!((end.hour(), end.minute()), (23, 59));
        // Always ahead of "Today", never behind it.
        assert!(expires_at(base(), 5) >= expires_at(base(), 4));
    }

    #[test]
    fn every_suggestion_names_a_real_duration() {
        for (_, shortcode, _, duration) in SUGGESTIONS {
            assert!(*duration < DURATIONS.len(), "{shortcode}");
            // And an emoji that actually draws.
            assert_ne!(crate::emoji::label(shortcode), format!(":{shortcode}:"));
        }
    }
}
