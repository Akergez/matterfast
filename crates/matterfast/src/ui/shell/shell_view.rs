use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{Context, FocusHandle, Subscription, Window};

use super::stage::Stage;
use crate::ui::{Action, MenuAction, Ui};

pub struct Shell {
    pub(super) stage: Stage,
    pub(super) focus: FocusHandle,
    /// What a sign-in failure during startup has to say; shown on the form.
    pub(super) notice: RefCell<Option<String>>,
    /// The composers and the search box of the current session, kept alive
    /// for as long as this window shows it.
    pub(super) session_subscriptions: Vec<Subscription>,
    /// The desktop switching between light and dark, which a "System" theme
    /// has to follow while the window is open.
    _appearance: Subscription,
}

impl Shell {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Only now is there a window to ask what the desktop looks like.
        crate::appearance::apply(Some(window), cx);
        let appearance = window
            .observe_window_appearance(|window, cx| crate::appearance::apply(Some(window), cx));
        // Android's window announces a change of appearance only when it
        // differs from what the window itself believes, and it starts out
        // believing light, so a switch back to light goes unannounced. Every
        // change of configuration is announced as a change of keyboard layout,
        // though — from inside the platform's own lock, hence the task.
        #[cfg(target_os = "android")]
        cx.on_keyboard_layout_change(|cx| {
            cx.spawn(async move |cx| {
                let _ = cx.update(|cx| {
                    // A new wallpaper is a change of configuration too, and
                    // the colours the phone makes of it are not announced.
                    gpui_adaptive_colors::refresh(cx);
                    if crate::appearance::behind_the_system(cx) {
                        crate::appearance::apply(None, cx);
                        crate::ui::refresh(cx);
                    }
                });
            })
            .detach();
        })
        .detach();
        Shell {
            _appearance: appearance,
            stage: Stage::Loading,
            focus: cx.focus_handle(),
            notice: RefCell::new(None),
            session_subscriptions: Vec::new(),
        }
    }

    /// The session, when that is what the window is showing.
    pub(super) fn session(&self) -> Option<&Rc<Ui>> {
        match &self.stage {
            Stage::Session(ui, _) => Some(ui),
            _ => None,
        }
    }

    pub(super) fn menu(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        if let Some(ui) = self.session() {
            ui.later(cx, move |ui, cx| ui.menu_action(action, cx));
        }
    }

    /// Escape, when nothing more specific wanted it: puts away whatever is
    /// in front. Answers whether there was anything to put away.
    pub(super) fn close_front(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ui) = self.session().cloned() else {
            return false;
        };
        if crate::ui::lightbox::dismiss(&ui, cx) {
            return true;
        }
        if ui.overlay.shown() {
            ui.dispatch(Action::CloseRightPanel, cx);
            return true;
        }
        false
    }
}
