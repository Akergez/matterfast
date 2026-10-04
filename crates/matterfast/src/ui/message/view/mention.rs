use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::text::{
    markdown_ast, InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext,
    MarkdownPlugin,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, ElementId, FontWeight, SharedString, Window};

/// Draws a mention as something to point at and press, rather than as a link.
///
/// [`crate::markdown`] writes one as a link under its own scheme; this claims
/// those links. Pointing at one shows who it is, pressing it opens their
/// profile — and the ones that are about you are marked, which is the only
/// part of a long message most people are looking for.
pub(super) struct Mention;

/// A mention, carried from parsing to drawing.
struct MentionOf {
    /// The username, a group's name, or one of `here`, `channel`, `all`.
    handle: String,
    /// What the message shows: the person's name as this reader has asked
    /// for names to be shown.
    label: SharedString,
}

/// What a mention of a whole audience does, for the card that explains it.
pub(crate) fn audience(handle: &str) -> Option<&'static str> {
    match handle {
        "here" => Some("Notifies everyone in this channel who is online"),
        "channel" | "all" => Some("Notifies everyone in this channel"),
        _ => None,
    }
}

impl MarkdownPlugin for Mention {
    fn name(&self) -> &str {
        "mention"
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        _: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Link(link) = node else {
            return None;
        };
        let handle = link.url.strip_prefix(crate::markdown::MENTION_SCHEME)?;
        let label: SharedString = node.to_string().into();
        Some(
            MarkdownNode::new(
                self.name().to_string(),
                MentionOf {
                    handle: handle.to_string(),
                    label: label.clone(),
                },
            )
            .text(label)
            // Copying a message gives the handle back, as it was typed.
            .markdown(format!("@{handle}")),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let MentionOf { handle, label } = node.data::<MentionOf>()?;
        let ui = crate::ui::current(cx)?;
        let theme = cx.theme();
        // Yours, or everyone's and therefore yours too.
        let about_me = audience(handle).is_some() || ui.state.borrow().me.username == *handle;
        let (wash, ink) = if about_me {
            (theme.warning.opacity(0.28), theme.foreground)
        } else {
            (theme.info.opacity(0.14), theme.info)
        };
        let hover = if about_me {
            theme.warning.opacity(0.42)
        } else {
            theme.info.opacity(0.26)
        };

        let at = node.source_range().map_or(0, |range| range.start);
        let pill = div()
            .id(ElementId::Name(format!("mention-{handle}-{at}").into()))
            .px(px(3.))
            .rounded_sm()
            .text_size(context.font_size())
            .line_height(context.line_height())
            .font_weight(FontWeight::MEDIUM)
            .bg(wash)
            .text_color(ink)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(label.clone())
            .on_click({
                let (ui, handle) = (ui.clone(), handle.clone());
                move |_, _, cx| {
                    cx.stop_propagation();
                    let handle = handle.clone();
                    ui.later(cx, move |ui, cx| ui.show_profile_by_handle(&handle, cx));
                }
            });

        let handle = handle.clone();
        Some(InlineElement::new(
            HoverCard::new(ElementId::Name(format!("mention-card-{handle}-{at}").into()))
                .open_delay(std::time::Duration::from_millis(350))
                .trigger(pill)
                .content(move |_, _, cx| match crate::ui::current(cx) {
                    Some(ui) => crate::ui::profile::glance(&ui, &handle, cx),
                    None => div().into_any_element(),
                }),
        ))
    }
}
