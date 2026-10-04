//! What the session does with its window: deferring work until the window is
//! free, asking it for things, and attaching and detaching it.

use std::rc::Rc;

use gpui_kit::component::notification::Notification;
use gpui_kit::component::WindowExt;
use gpui_kit::{AnyWindowHandle, App, ClickEvent, Window};

use super::ui::Ui;
use crate::ui::Action;

impl Ui {
    /// Runs `f` once whatever is running now has finished. This is how a
    /// click, which happens in the middle of the window drawing itself,
    /// reaches session code that may want that window.
    pub fn later(self: &Rc<Self>, cx: &mut App, f: impl FnOnce(&Rc<Ui>, &mut App) + 'static) {
        let ui = self.clone();
        cx.defer(move |cx| f(&ui, cx));
    }

    /// A click handler that runs `f` with the session, later.
    pub fn click(
        self: &Rc<Self>,
        f: impl Fn(&Rc<Ui>, &mut App) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let ui = self.clone();
        let f = Rc::new(f);
        move |_, _, cx| {
            let ui = ui.clone();
            let f = f.clone();
            cx.defer(move |cx| f(&ui, cx));
        }
    }

    /// Runs `f` with the window, if there is one.
    pub fn with_window<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        self.window.update(cx, f)
    }

    /// The window's scale factor, as last drawn.
    pub fn scale_factor(&self) -> f32 {
        self.scale.get()
    }

    /// Whether the window is in front of the person right now.
    pub(crate) fn window_is_active(&self, cx: &mut App) -> bool {
        self.window
            .update(cx, |window, _| window.is_window_active())
            .unwrap_or(false)
    }

    /// A window was built around this session.
    pub(crate) fn attach(self: &Rc<Self>, window: AnyWindowHandle, cx: &mut App) {
        self.window.set(Some(window));
        self.refresh_title(cx);
        self.restore_draft(cx);
        self.restore_thread_draft(cx);
    }

    /// The window is going away; the session is not.
    pub(crate) fn detach(&self, cx: &mut App) {
        // The debounce means the last few seconds would otherwise be lost,
        // and closing the window is exactly when the next launch's picture
        // is decided.
        self.save_snapshot(cx);
        self.window.set(None);
        self.chat.detach();
        self.right.detach();
        self.search_box.detach();
    }

    /// Says something that happened, where it will be seen and then go away.
    pub fn toast(&self, message: &str, cx: &mut App) {
        let message = message.to_string();
        self.window.update(cx, move |window, cx| {
            window.push_notification(Notification::new().message(message.clone()), cx);
        });
    }

    /// Queues an action. It runs once whatever is running now has finished —
    /// never inside it — so the code that dispatches it does not have to care
    /// what state it is in the middle of changing.
    pub fn dispatch(self: &Rc<Self>, action: Action, cx: &mut App) {
        let ui = self.clone();
        cx.defer(move |cx| ui.handle(action, cx));
    }
}
