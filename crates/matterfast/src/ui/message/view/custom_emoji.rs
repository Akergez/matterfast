use gpui_kit::component::text::{
    markdown_ast, InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext,
    MarkdownPlugin,
};
use gpui_kit::prelude::*;
use gpui_kit::{img, App, ElementId, ObjectFit, Window};

/// Draws the server's own emoji inside a line of text.
///
/// [`crate::markdown::prepare_full`] writes one as an image addressed by
/// name; this claims those images, so they are drawn from the picture cache
/// — which holds the session — instead of being fetched as a URL.
pub(super) struct CustomEmoji;

/// A custom emoji's name, carried from parsing to drawing.
struct EmojiName(String);

impl MarkdownPlugin for CustomEmoji {
    fn name(&self) -> &str {
        "custom-emoji"
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        _: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Image(image) = node else {
            return None;
        };
        let name = image.url.strip_prefix(crate::markdown::EMOJI_SCHEME)?;
        // Copying a message gives the shortcode back, as it was typed.
        let shortcode = format!(":{name}:");
        Some(
            MarkdownNode::new(self.name().to_string(), EmojiName(name.to_string()))
                .text(shortcode.clone())
                .markdown(shortcode),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let EmojiName(name) = node.data::<EmojiName>()?;
        // Not here yet, or not on this server after all: `None` leaves the
        // shortcode standing as text, and asking is what starts the fetch.
        let picture = crate::ui::current(cx)?.avatars.custom_emoji(name)?;
        // As tall as the line, which is a little taller than the letters:
        // a picture the height of an "x" is too small to make out.
        let size = context.line_height();
        // Named by where it sits in the message: an animation keeps the
        // frame it is on under its id, and one without an id never leaves
        // its first.
        let at = node.source_range().map_or(0, |range| range.start);
        Some(InlineElement::new(
            img(picture)
                .id(ElementId::Name(format!("emoji-{name}-{at}").into()))
                .size(size)
                .object_fit(ObjectFit::Contain),
        ))
    }
}
