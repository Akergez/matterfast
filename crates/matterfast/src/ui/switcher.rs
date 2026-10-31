//! The quick switcher: Ctrl+K, type, Enter.
//!
//! Like [`super::dialogs::ChannelBrowser`] it only asks and answers — the
//! caller searches its own state on `on_search` and pours the matches back in
//! through [`Switcher::set_results`], so nothing here knows what a channel is.
//!
//! The whole point is that your hands stay on the keys, so the search box
//! owns the focus for the dialog's whole life: Up and Down move the selection
//! in the list without ever giving it focus.

use gpui_adaptive::prim::{column, row, Scrolls as _};
use gpui_adaptive::{ActiveColors as _, FieldEvent, FieldKey, Purpose, Sheet, TextField, TextFieldState};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, ScrollHandle, Subscription, Window};

use super::autocomplete::next_index;
use super::kit::Lucide;
use super::Ui;

/// What was picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Channel(String),
    User(String),
}

/// One row of results.
#[derive(Clone)]
pub struct Entry {
    pub target: Target,
    pub title: String,
    pub subtitle: String,
    pub icon: Lucide,
}

struct View {
    search: Entity<TextFieldState>,
    entries: Vec<Entry>,
    /// Something is always selected, so Enter answers with the best match
    /// straight after typing, without a press spent picking a start.
    selected: usize,
    scroll: ScrollHandle,
    on_pick: Rc<dyn Fn(Target, &mut App)>,
    _subscription: Subscription,
}

impl View {
    /// Moves the selection, wrapping at both ends: a screenful of results is
    /// quicker to circle than to walk back through.
    fn step(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.selected = next_index(self.selected, delta, self.entries.len());
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        let target = entry.target.clone();
        let on_pick = self.on_pick.clone();
        Sheet::close(window, cx);
        cx.defer(move |cx| on_pick(target, cx));
    }
}

impl Render for View {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let mut rows = column()
            .id("switcher-results")
            .max_h(px(360.))
            .gap_0p5()
            .track_scroll(&self.scroll)
            .scrolls_y();
        if self.entries.is_empty() {
            rows = rows.child(
                div()
                    .py_6()
                    .text_center()
                    .text_color(colors.muted)
                    .child("No matches"),
            );
        }
        for (index, entry) in self.entries.iter().enumerate() {
            rows = rows.child(
                row()
                    .id(("result", index))
                    .gap_2()
                    .px_2()
                    .h(px(36.))
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .when(index == self.selected, |row| row.bg(colors.hover))
                    .hover(|style| style.bg(colors.hover))
                    .child(match &entry.target {
                        Target::Channel(_) => {
                            super::channel_avatar::named(&entry.title, 24.).into_any_element()
                        }
                        Target::User(_) => div()
                            .text_color(colors.muted)
                            .child(entry.icon)
                            .into_any_element(),
                    })
                    .child(div().flex_1().min_w_0().truncate().child(entry.title.clone()))
                    .when(!entry.subtitle.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_none()
                                .text_sm()
                                .text_color(colors.muted)
                                .child(entry.subtitle.clone()),
                        )
                    })
                    // Clicking a row answers the same way Enter does.
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.pick(index, window, cx)
                    })),
            );
        }

        // Up and Down are taken by the list, so that it never takes the focus
        // off the box: everything else is typing, and typing belongs to it.
        let view = cx.entity().downgrade();
        let search = TextField::new(&self.search).icon(Lucide::Search).on_key(move |key, _, cx| {
            let delta = match key {
                FieldKey::Up => -1,
                FieldKey::Down => 1,
                _ => return false,
            };
            view.update(cx, |view, cx| view.step(delta, cx)).is_ok()
        });
        column().gap_2().child(search).child(rows)
    }
}

/// The dialog outlives the call that opened it; the handle is how results
/// get in.
pub struct Switcher {
    view: Option<Entity<View>>,
}

impl Switcher {
    /// `on_search` fires as the person types (already the trimmed term).
    /// `on_pick` fires with the chosen target and the dialog closes itself.
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        on_search: impl Fn(String, &mut App) + 'static,
        on_pick: impl Fn(Target, &mut App) + 'static,
    ) -> Rc<Self> {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let on_pick: Rc<dyn Fn(Target, &mut App)> = Rc::new(on_pick);
        let view = ui.with_window(cx, move |window, cx| {
            let view = cx.new(|cx| {
                let search = cx.new(|cx| {
                    let mut search = TextFieldState::new(Purpose::Plain, window, cx);
                    search.set_placeholder("Jump to a channel or person", window, cx);
                    search
                });
                let subscription = cx.subscribe_in(
                    &search,
                    window,
                    move |view: &mut View, search, event: &FieldEvent, window, cx| match event {
                        FieldEvent::Change => {
                            let term = search.read(cx).value(cx).trim().to_string();
                            let typed = typed.clone();
                            cx.defer(move |cx| typed(term, cx));
                        }
                        FieldEvent::Submit => view.pick(view.selected, window, cx),
                        _ => {}
                    },
                );
                View {
                    search,
                    entries: Vec::new(),
                    selected: 0,
                    scroll: ScrollHandle::new(),
                    on_pick,
                    _subscription: subscription,
                }
            });
            let search = view.read(cx).search.clone();
            let options = gpui_adaptive::SheetOptions {
                width: Some(gpui_kit::rems(32.5)),
                no_close_button: true,
                ..Default::default()
            };
            Sheet::open_with(view.clone(), options, window, cx);
            search.update(cx, |search, cx| search.focus(window, cx));
            view
        });
        // Ask once on open: the conversations you have are worth showing
        // before anyone types, and an empty dialog looks broken.
        cx.defer(move |cx| on_search(String::new(), cx));
        Rc::new(Switcher { view })
    }

    /// What is in the search box right now.
    pub fn term(&self, cx: &App) -> String {
        self.view
            .as_ref()
            .map(|view| view.read(cx).search.read(cx).value(cx).trim().to_string())
            .unwrap_or_default()
    }

    pub fn set_results(&self, entries: Vec<Entry>, cx: &mut App) {
        let Some(view) = &self.view else { return };
        view.update(cx, |view, cx| {
            view.entries = entries;
            view.selected = 0;
            cx.notify();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wraps_at_both_ends() {
        assert_eq!(next_index(0, 1, 3), 1);
        assert_eq!(next_index(2, 1, 3), 0);
        assert_eq!(next_index(0, -1, 3), 2);
        assert_eq!(next_index(1, -1, 3), 0);
        // One result: both keys stay on it.
        assert_eq!(next_index(0, 1, 1), 0);
    }

    #[test]
    fn nothing_to_move_through_stays_on_the_first_row() {
        // No rows at all: the caller finds no row 0 and gives up.
        assert_eq!(next_index(0, 1, 0), 0);
        assert_eq!(next_index(0, -1, 0), 0);
    }
}
