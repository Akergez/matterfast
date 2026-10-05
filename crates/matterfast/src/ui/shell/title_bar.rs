use gpui_kit::component::{h_flex, ActiveTheme, Selectable, TitleBar};
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, AnyElement, Context, FontWeight, MouseButton, Window};

use super::app_icon::app_icon;
use super::constants::SIDEBAR_PAGE_BELOW;
use super::column_width::column_width;
use super::pretty_server::pretty_server;
use super::shell_view::Shell;
use crate::ui::kit::{self, Lucide};
use crate::ui::rhs::PanelMode;
use crate::ui::MenuAction;

impl Shell {
    /// The window's own header: what this is and which server it is talking
    /// to on the left, the way to anywhere in the middle, what is waiting for
    /// you on the right. The bar under all of it is what drags the window, so
    /// everything that takes a click keeps the press to itself.
    ///
    /// It is painted as the channel sidebar is: the two are one frame around
    /// the conversation, in one colour, and everything else is the page.
    pub(super) fn title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let width = f32::from(window.viewport_size().width);
        let roomy = width >= SIDEBAR_PAGE_BELOW;
        // A window that is not the one being typed into says so quietly.
        let active = window.is_window_active();

        let mut name = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(img(app_icon()).flex_none().size(px(18.)))
            .child(
                div()
                    .flex_none()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(!active, |name| name.text_color(theme.muted_foreground))
                    .child("Matterfast"),
            );
        let mut actions = h_flex()
            .flex_1()
            .min_w_0()
            .pr_1()
            .gap_1()
            .items_center()
            .justify_end();
        let mut bar = h_flex().size_full().gap_3().items_center();

        // The toolkit's bar is a gradient of its own colour; a flat fill in
        // the sidebar's is what makes the two read as one surface.
        #[cfg(not(target_os = "android"))]
        let framed = || {
            TitleBar::new()
                .bg(theme.sidebar)
                .border_color(theme.sidebar_border)
                .text_color(theme.sidebar_foreground)
        };
        // A phone has no window to drag, minimise or close, and the toolkit's
        // bar draws the buttons for that regardless: a plain strip instead,
        // tall enough for a finger.
        #[cfg(target_os = "android")]
        let framed = || {
            h_flex()
                .flex_none()
                .h(px(44.))
                .px_3()
                .border_b_1()
                .bg(theme.sidebar)
                .border_color(theme.sidebar_border)
                .text_color(theme.sidebar_foreground)
        };

        let Some(ui) = self.session().cloned() else {
            return framed().child(bar.child(name)).into_any_element();
        };
        let server = pretty_server(ui.state.borrow().client.site_url());
        // Counted where the window's title is, not here: see `Ui::mentions`.
        let mentions = ui.mentions.get();
        if roomy {
            name = name.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(server),
            );
        }

        // Searching messages is the one thing done from anywhere, so it has
        // the middle of the bar; going to a channel is done from the list of
        // channels, and its button is there.
        let search = crate::ui::search::render(
            &ui,
            column_width(width, 0.32, 150.0, 380.0),
            window,
            cx,
        );

        let inbox_open =
            ui.overlay.shown() && matches!(ui.right.mode(cx), PanelMode::Inbox);
        actions = actions.child(
            h_flex()
                .id("title-inbox")
                .flex_none()
                .gap_1()
                .items_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when(mentions > 0, |inbox| {
                    inbox.child(kit::mention_badge(mentions, false, cx))
                })
                .child(
                    kit::icon_button("title-inbox-button", Lucide::Inbox, "Inbox")
                        .selected(inbox_open)
                        .on_click(cx.listener(|shell, _, _, cx| {
                            cx.stop_propagation();
                            shell.menu(MenuAction::OpenInbox, cx)
                        })),
                ),
        );

        bar = bar.child(name).child(search).child(actions);
        framed().child(bar).into_any_element()
    }
}
