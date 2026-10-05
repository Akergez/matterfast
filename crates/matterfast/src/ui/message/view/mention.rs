use gpui_kit::component::text::{
    markdown_ast, InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext,
    MarkdownPlugin,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, ElementId, FontWeight, SharedString, Window};

/// Marks the mentions that are about the reader, which is the only part of a
/// long message most people are looking for.
///
/// [`crate::markdown`] writes every mention as a link under its own scheme,
/// and a link is what most of them stay: pressing one opens the profile, as
/// following any link of that scheme does. They used to be drawn as elements,
/// each with a card that opened under the pointer, and an element inside a
/// line of text is laid out by itself on every frame — in a conversation that
/// names people, that was a sixth of the time a frame took. Only the ones
/// [`mark_mine`] has picked out are claimed here and drawn as a pill.
pub(super) struct Mention;

/// The scheme of a mention that is about the reader.
const MINE_SCHEME: &str = "mm-mention-me:";

/// Moves the mentions of `me`, and of everybody, to the scheme [`Mention`]
/// claims. Done to the prepared text rather than while parsing it, because a
/// parser is not told who is reading.
pub(crate) fn mark_mine(markdown: String, me: &str) -> String {
    let scheme = crate::markdown::MENTION_SCHEME;
    let mut marked = markdown;
    for handle in [me, "here", "channel", "all"] {
        // Up to the closing bracket, so that `anna` does not claim `annabel`.
        let written = format!("]({scheme}{handle})");
        if !handle.is_empty() && marked.contains(&written) {
            marked = marked.replace(&written, &format!("]({MINE_SCHEME}{handle})"));
        }
    }
    marked
}

/// A mention, carried from parsing to drawing.
struct MentionOf {
    /// The username, a group's name, or one of `here`, `channel`, `all`.
    handle: String,
    /// What the message shows: the person's name as this reader has asked
    /// for names to be shown.
    label: SharedString,
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
        let handle = link.url.strip_prefix(MINE_SCHEME)?;
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
        let (wash, ink) = (theme.warning.opacity(0.28), theme.foreground);
        let hover = theme.warning.opacity(0.42);

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

        Some(InlineElement::new(pill))
    }
}

#[cfg(test)]
mod tests {
    use super::mark_mine;

    #[test]
    fn a_mention_of_the_reader_and_of_everybody_is_marked_and_nobody_elses_is() {
        let prepared = "[@Anna](mm-mention:anna) and [@Annabel](mm-mention:annabel), \
                        [@here](mm-mention:here)"
            .to_string();
        assert_eq!(
            mark_mine(prepared, "anna"),
            "[@Anna](mm-mention-me:anna) and [@Annabel](mm-mention:annabel), \
             [@here](mm-mention-me:here)"
        );
    }

    #[test]
    fn a_reader_without_a_name_yet_claims_nothing_but_the_mentions_of_everybody() {
        let prepared = "[@Anna](mm-mention:anna) [@all](mm-mention:all)".to_string();
        assert_eq!(
            mark_mine(prepared, ""),
            "[@Anna](mm-mention:anna) [@all](mm-mention-me:all)"
        );
    }
}
