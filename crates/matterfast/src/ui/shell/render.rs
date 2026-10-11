use gpui_kit::component::input::Escape;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{v_flex, ActiveTheme, Sizable, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Context, Focusable, Window};

use super::actions::{
    ClosePanel, FocusSearch, NewChannel, NextUnread, OpenInbox, OpenSettings, PreviousUnread,
    QuickSwitch, CONTEXT,
};
use super::choose_server::choose_server;
use super::constants::SIDEBAR_PAGE_BELOW;
use super::session_panes::session;
use super::shell_view::Shell;
use super::stage::Stage;
use super::system_bars::system_bars;
use crate::ui::MenuAction;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ui::frame_log::begin();
        let theme = cx.theme().clone();
        let notice = self.notice.borrow_mut().take();
        if let Some(notice) = notice {
            window.push_notification(
                gpui_kit::component::notification::Notification::new().message(notice),
                cx,
            );
        }

        let phone_session = self.session().is_some()
            && f32::from(window.viewport_size().width) < SIDEBAR_PAGE_BELOW;
        let phone_panel = phone_session && self.session().is_some_and(|ui| {
            ui.overlay.shown()
                && !matches!(ui.right.mode(cx), crate::ui::rhs::PanelMode::Hidden)
        });
        if self.phone_panel_shown != phone_panel {
            self.phone_panel_shown = phone_panel;
            if phone_panel {
                self.phone_panel_restore_focus = window.focused(cx);
                let reply_focused = self.session().is_some_and(|ui| {
                    ui.right.composer.borrow().as_ref().is_some_and(|reply| {
                        reply.read(cx).focus_handle(cx).contains_focused(window, cx)
                    })
                });
                // The covered search/composer leaves the focus tree with its
                // page. Keep Escape on the shell unless the visible reply
                // box already owns focus. This runs only on page transitions.
                if !reply_focused {
                    window.focus(&self.focus, cx);
                } else {
                    // The reply will itself disappear on close, so it cannot
                    // be the restore target for the covered page.
                    self.phone_panel_restore_focus = Some(self.focus.clone());
                }
            } else {
                let restore = self.phone_panel_restore_focus.take();
                if self.session().is_some() {
                    if let Some(focus) = restore {
                        window.focus(&focus, cx);
                    }
                }
            }
        }
        let title_bar = (!phone_session).then(|| self.title_bar(window, cx));
        let (bars_top, bars_bottom) = system_bars();
        let body = match &self.stage {
            Stage::Loading => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large())
                .into_any_element(),
            Stage::Login(view) => view.clone().into_any_element(),
            Stage::ChooseServer(servers) => choose_server(&servers.clone(), cx),
            Stage::Session(ui, panes) => session(&ui.clone(), panes, window, cx),
        };

        v_flex()
            .id("matterfast")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|shell, _: &QuickSwitch, _, cx| {
                shell.menu(MenuAction::QuickSwitch, cx)
            }))
            .on_action(cx.listener(|shell, _: &FocusSearch, _, cx| {
                shell.menu(MenuAction::FocusSearch, cx)
            }))
            .on_action(cx.listener(|shell, _: &OpenInbox, _, cx| {
                shell.menu(MenuAction::OpenInbox, cx)
            }))
            .on_action(cx.listener(|shell, _: &NewChannel, _, cx| {
                shell.menu(MenuAction::NewChannel, cx)
            }))
            .on_action(cx.listener(|shell, _: &OpenSettings, _, cx| {
                shell.menu(MenuAction::Settings, cx)
            }))
            .on_action(cx.listener(|shell, _: &NextUnread, _, cx| {
                shell.menu(MenuAction::NextUnread, cx)
            }))
            .on_action(cx.listener(|shell, _: &PreviousUnread, _, cx| {
                shell.menu(MenuAction::PreviousUnread, cx)
            }))
            .on_action(cx.listener(|shell, _: &ClosePanel, _, cx| {
                if !shell.close_front(cx) {
                    cx.propagate();
                }
            }))
            // The composer is where the focus nearly always is, and it has
            // its own idea of what Escape means. A picture over the window,
            // or an open panel, outranks that; a dialog or a completion list
            // does not, and gets the key as usual.
            .capture_action(cx.listener(|shell, _: &Escape, window, cx| {
                let busy = window.has_active_dialog(cx)
                    || shell
                        .session()
                        .is_some_and(|ui| ui.chat.completing() || ui.right.completing());
                if !busy && shell.close_front(cx) {
                    cx.stop_propagation();
                }
            }))
            // Under the status bar, in the title bar's colour so that the two
            // read as one.
            .child(div().flex_none().h(px(bars_top)).w_full().bg(theme.sidebar))
            .when_some(title_bar, |shell, title| {
                shell.child(crate::ui::frame_log::timed("title", title))
            })
            .child(div().flex_1().min_h_0().w_full().child(body))
            .pb(px(bars_bottom))
            .when(crate::ui::frame_log::enabled(), |shell| {
                shell.child(crate::ui::frame_log::end())
            })
    }
}
