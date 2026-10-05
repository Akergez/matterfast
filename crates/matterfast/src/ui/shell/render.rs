use gpui_kit::component::input::Escape;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{v_flex, ActiveTheme, Sizable, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Context, Window};

use super::actions::{
    ClosePanel, FocusSearch, NewChannel, NextUnread, OpenInbox, OpenSettings, PreviousUnread,
    QuickSwitch, CONTEXT,
};
use super::choose_server::choose_server;
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

        let title_bar = self.title_bar(window, cx);
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
                    || shell.session().is_some_and(|ui| ui.chat.completing());
                if !busy && shell.close_front(cx) {
                    cx.stop_propagation();
                }
            }))
            // Under the status bar, in the title bar's colour so that the two
            // read as one.
            .child(div().flex_none().h(px(bars_top)).w_full().bg(theme.sidebar))
            .child(title_bar)
            .child(div().flex_1().min_h_0().w_full().child(body))
            .pb(px(bars_bottom))
            .when(crate::ui::frame_log::enabled(), |shell| {
                shell.child(crate::ui::frame_log::end())
            })
    }
}
