//! The quick switcher: Ctrl+K, type, Enter.
//!
//! Like [`super::dialogs::ChannelBrowser`] it only asks and answers — the
//! caller searches its own state on `on_search` and pours the matches back in
//! through [`Switcher::set_results`], so nothing here knows what a channel is.
//!
//! The whole point is that your hands stay on the keys, so the search box
//! owns the focus for the dialog's whole life: Up and Down move the selection
//! in the list without ever giving it focus.

use std::rc::Rc;

use gpui_kit::component::input::{Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, WindowExt};
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
    search: Entity<InputState>,
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
        window.close_dialog(cx);
        cx.defer(move |cx| on_pick(target, cx));
    }
}

impl Render for View {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut rows = v_flex()
            .id("switcher-results")
            .max_h(px(360.))
            .gap_0p5()
            .track_scroll(&self.scroll)
            .overflow_y_scroll();
        if self.entries.is_empty() {
            rows = rows.child(
                div()
                    .py_6()
                    .text_center()
                    .text_color(theme.muted_foreground)
                    .child("No matches"),
            );
        }
        for (index, entry) in self.entries.iter().enumerate() {
            rows = rows.child(
                h_flex()
                    .id(("result", index))
                    .gap_2()
                    .px_2()
                    .h(px(36.))
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .when(index == self.selected, |row| row.bg(theme.accent))
                    .hover(|style| style.bg(theme.list_hover))
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(entry.icon.clone()),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(entry.title.clone()))
                    .when(!entry.subtitle.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_none()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(entry.subtitle.clone()),
                        )
                    })
                    // Clicking a row answers the same way Enter does.
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.pick(index, window, cx)
                    })),
            );
        }

        v_flex()
            .gap_2()
            // Swallowed here so the list never takes the focus off the box:
            // everything else is typing, and typing belongs to it.
            .capture_action(cx.listener(|view, _: &MoveUp, _, cx| {
                view.step(-1, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|view, _: &MoveDown, _, cx| {
                view.step(1, cx);
                cx.stop_propagation();
            }))
            .child(Input::new(&self.search).prefix(Lucide::Search))
            .child(rows)
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
                    InputState::new(window, cx).placeholder("Jump to a channel or person")
                });
                let subscription = cx.subscribe_in(
                    &search,
                    window,
                    move |view: &mut View, search, event: &InputEvent, window, cx| match event {
                        InputEvent::Change => {
                            let term = search.read(cx).value().trim().to_string();
                            let typed = typed.clone();
                            cx.defer(move |cx| typed(term, cx));
                        }
                        InputEvent::PressEnter { .. } => view.pick(view.selected, window, cx),
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
            let body = view.clone();
            window.open_dialog(cx, move |dialog, _, _| {
                dialog.w(px(520.)).close_button(false).child(body.clone())
            });
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
            .map(|view| view.read(cx).search.read(cx).value().trim().to_string())
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
