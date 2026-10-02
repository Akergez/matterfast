//! Storage preferences for the bounded HTTP/media cache.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, Subscription, Window};

use super::Ui;
use crate::resource_cache::ResourceCache;
use crate::runtime;

const GIB: u64 = 1024 * 1024 * 1024;

/// The largest limit that can be set, in GiB.
const MAX_GIB: f64 = 50.0;

struct Storage {
    cache: ResourceCache,
    limit: Entity<InputState>,
    /// What the cache holds right now, once it has been measured.
    used: Option<u64>,
    clearing: bool,
    _subscription: Subscription,
}

impl Storage {
    fn measure(&mut self, cx: &mut Context<Self>) {
        let cache = self.cache.clone();
        let view = cx.entity();
        runtime::spawn(async move { cache.size().await }, move |bytes, cx| {
            view.update(cx, |storage, cx| {
                storage.used = Some(bytes);
                cx.notify();
            });
        });
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.clearing = true;
        cx.notify();
        let cache = self.cache.clone();
        let view = cx.entity();
        runtime::spawn(async move { cache.clear().await }, move |(), cx| {
            view.update(cx, |storage, cx| {
                storage.used = Some(0);
                storage.clearing = false;
                cx.notify();
            });
        });
    }
}

impl Render for Storage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let row = |title: &'static str, subtitle: String| {
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().child(title))
                .child(div().text_xs().text_color(muted).child(subtitle))
        };
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row(
                        "Media cache limit, GiB",
                        "Avatars, previews and opened attachments; 0 disables caching".into(),
                    ))
                    .child(div().w(px(96.)).child(Input::new(&self.limit))),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row(
                        "Currently used",
                        match self.used {
                            Some(bytes) => human_bytes(bytes),
                            None => "Calculating…".to_string(),
                        },
                    ))
                    .child(
                        Button::new("clear")
                            .label("Clear")
                            .small()
                            .danger()
                            .disabled(self.clearing)
                            .on_click(cx.listener(|storage, _, _, cx| storage.clear(cx))),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("The limit is shared by all accounts; cached files expire automatically."),
            )
    }
}

pub fn show(ui: &Rc<Ui>, cache: ResourceCache, cx: &mut App) {
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let current = crate::background::cache_limit_bytes() as f64 / GIB as f64;
            let limit =
                cx.new(|cx| InputState::new(window, cx).default_value(format!("{current:.2}")));
            let subscription = cx.subscribe(
                &limit,
                |storage: &mut Storage, limit, event: &InputEvent, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    // Only what reads as a number is a limit; a half-typed
                    // one changes nothing yet.
                    if let Some(bytes) = parse_limit(&limit.read(cx).value()) {
                        crate::background::set_cache_limit_bytes(bytes);
                        storage.cache.set_limit(bytes);
                    }
                },
            );
            let mut storage = Storage {
                cache,
                limit,
                used: None,
                clearing: false,
                _subscription: subscription,
            };
            storage.measure(cx);
            storage
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Storage").child(view.clone())
        });
    });
}

/// A typed limit in GiB, as bytes. Nothing for text that is not a number in
/// range.
fn parse_limit(text: &str) -> Option<u64> {
    let gib: f64 = text.trim().replace(',', ".").parse().ok()?;
    (0.0..=MAX_GIB)
        .contains(&gib)
        .then(|| (gib * GIB as f64).round() as u64)
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024 * 1024) as f64)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_usage_for_people() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.00 GiB");
    }

    #[test]
    fn a_limit_is_a_number_of_gib_within_range() {
        assert_eq!(parse_limit("5"), Some(5 * GIB));
        assert_eq!(parse_limit(" 0.25 "), Some(GIB / 4));
        // A comma is how half the world types a decimal.
        assert_eq!(parse_limit("1,5"), Some(GIB + GIB / 2));
        assert_eq!(parse_limit("0"), Some(0));
        assert_eq!(parse_limit(""), None);
        assert_eq!(parse_limit("lots"), None);
        assert_eq!(parse_limit("-1"), None);
        assert_eq!(parse_limit("51"), None);
    }
}
