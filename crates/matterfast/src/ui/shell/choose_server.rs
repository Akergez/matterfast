use gpui_adaptive::prim::column;
use gpui_adaptive::{ActiveColors as _, Command};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, Context, FontWeight};

use super::pretty_server::pretty_server;
use super::shell_view::Shell;
use crate::ui::kit::Lucide;

/// The list of stored servers to pick from.
pub(super) fn choose_server(servers: &[(String, String)], cx: &mut Context<Shell>) -> AnyElement {
    let colors = cx.colors();
    let mut list = column().w(px(420.)).max_w_full().gap_1().child(
        div()
            .pb_2()
            .text_xl()
            .text_center()
            .font_weight(FontWeight::SEMIBOLD)
            .child("Choose a server"),
    );
    for (index, (server, token)) in servers.iter().enumerate() {
        let (server_url, token) = (server.clone(), token.clone());
        list = list.child(
            column()
                .id(("server", index))
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .cursor_pointer()
                .hover(|style| style.bg(colors.hover))
                .child(div().font_weight(FontWeight::MEDIUM).child(pretty_server(server)))
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.muted)
                        .child(server.clone()),
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.restore_session(server_url.clone(), token.clone(), window, cx)
                })),
        );
    }
    list = list.child(
        div().mt_2().child(
            Command::new("add-server", "Add Server…")
                .icon(Lucide::Plus)
                .on_trigger({
                    let shell = cx.weak_entity();
                    move |window, cx| {
                        shell.update(cx, |shell, cx| shell.show_login(window, cx)).ok();
                    }
                })
                .as_button(),
        ),
    );
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .child(list)
        .into_any_element()
}
