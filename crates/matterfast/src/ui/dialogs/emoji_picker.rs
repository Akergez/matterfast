use gpui_adaptive::prim::{self, column, row, Scrolls as _};
use gpui_adaptive::{FieldEvent, Purpose, Sheet, SheetOptions, TextField, TextFieldState};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, Subscription, Window};

use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// How many emoji the picker shows at once. Filling the whole table costs a
/// few thousand buttons, so it is a page of whatever the search ranks first.
const EMOJI_PAGE: usize = 120;

/// The full emoji table, under a search box.
struct EmojiPicker {
    ui: Rc<Ui>,
    search: Entity<TextFieldState>,
    on_pick: Rc<dyn Fn(String, &mut App)>,
    _subscription: Subscription,
}

impl Render for EmojiPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let term = self.search.read(cx).value(cx).to_string();
        let found = crate::emoji::search(&term, &self.ui.state.borrow().custom_emoji, EMOJI_PAGE);
        let mut grid = row().flex_wrap().gap_0p5();
        for found in found {
            let name = found.name().to_string();
            let on_pick = self.on_pick.clone();
            let button = prim::pressable(gpui_kit::ElementId::Name(format!("emoji-{name}").into()), cx)
                .size(px(32.))
                .justify_center()
                .rounded_md()
                .tooltip(prim::tooltip(format!(":{name}:")));
            let button = match &found {
                crate::emoji::Found::Unicode(_, glyph) => button.child(*glyph),
                // The server's own: its picture, the size of the glyphs
                // beside it.
                crate::emoji::Found::Custom(name) => {
                    button.child(crate::ui::message::emoji_element(&self.ui, name, 20.))
                }
            };
            grid = grid.child(prim::testable(button.on_click(move |_, window, cx| {
                Sheet::close(window, cx);
                let on_pick = on_pick.clone();
                let name = name.clone();
                cx.defer(move |cx| on_pick(name, cx));
            })));
        }
        column()
            .gap_2()
            .child(TextField::new(&self.search).icon(Lucide::Search).cleanable())
            .child(
                div()
                    .id("emoji-grid")
                    .h(px(260.))
                    .scrolls_y()
                    .child(grid),
            )
    }
}

/// Asks which emoji, from the whole table. Answers its shortcode.
pub fn pick_emoji(ui: &Rc<Ui>, cx: &mut App, on_pick: impl Fn(String, &mut App) + 'static) {
    let on_pick: Rc<dyn Fn(String, &mut App)> = Rc::new(on_pick);
    let ui = ui.clone();
    ui.clone().with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let search = cx.new(|cx| {
                let mut search = TextFieldState::new(Purpose::Plain, window, cx);
                search.set_placeholder("Search emoji", window, cx);
                search
            });
            let subscription = cx.subscribe(&search, |_, _, event: &FieldEvent, cx| {
                if *event == FieldEvent::Change {
                    cx.notify();
                }
            });
            EmojiPicker {
                ui: ui.clone(),
                search,
                on_pick,
                _subscription: subscription,
            }
        });
        let search = view.read(cx).search.clone();
        let options = SheetOptions { title: Some("Add reaction".into()), ..Default::default() };
        Sheet::open_with(view, options, window, cx);
        search.update(cx, |search, cx| search.focus(window, cx));
    });
}
