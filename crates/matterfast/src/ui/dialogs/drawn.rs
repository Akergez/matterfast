use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Render, Window};

/// What a sheet shows when it is drawn afresh from the session each time
/// (a person's card: their presence and picture change while it is up),
/// rather than kept in a view of its own.
pub(crate) struct Drawn {
    draw: Box<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl Drawn {
    pub(crate) fn new(draw: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        Drawn { draw: Box::new(draw) }
    }
}

impl Render for Drawn {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.draw)(window, cx)
    }
}
