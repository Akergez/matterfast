//! Mattermost messages are Markdown. This turns them into Pango markup.
//!
//! Not a renderer — a translator into what a `GtkLabel` can already draw.
//! Pango has no block layout, so anything structural (a code block, a table)
//! is left to the caller as a separate [`Block`], and only the inline span
//! markup is produced here.
//!
//! Everything that reaches Pango is escaped first. A message is arbitrary text
//! from another person, and `<span>` in someone's message must render as those
//! six characters and never as a tag.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// A message split into things a widget can be built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Pango markup for a run of prose.
    Text(String),
    /// A fenced or indented code block, and its language when it named one.
    Code { language: String, text: String },
}

/// Splits a message into blocks, translating inline Markdown into Pango markup.
pub fn parse(message: &str) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut code: Option<(String, String)> = None;
    // Nesting depth of list items, so a nested bullet is indented rather than
    // flattened onto the parent.
    let mut list_depth: usize = 0;
    let mut quote_depth: usize = 0;

    let flush = |text: &mut String, blocks: &mut Vec<Block>| {
        let trimmed = text.trim_matches('\n');
        if !trimmed.is_empty() {
            blocks.push(Block::Text(trimmed.to_string()));
        }
        text.clear();
    };

    for event in Parser::new_ext(message, options) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(&mut text, &mut blocks);
                let language = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((language, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((language, body)) = code.take() {
                    blocks.push(Block::Code {
                        language,
                        text: body.trim_end_matches('\n').to_string(),
                    });
                }
            }
            Event::Text(t) => match code.as_mut() {
                Some((_, body)) => body.push_str(&t),
                None => text.push_str(&inline(&t)),
            },
            // Inline code is a span, not a block: it belongs in the sentence.
            Event::Code(t) => {
                text.push_str("<tt><span background=\"#00000018\"> ");
                text.push_str(&escape(&t));
                text.push_str(" </span></tt>");
            }
            Event::Start(Tag::Strong) => text.push_str("<b>"),
            Event::End(TagEnd::Strong) => text.push_str("</b>"),
            Event::Start(Tag::Emphasis) => text.push_str("<i>"),
            Event::End(TagEnd::Emphasis) => text.push_str("</i>"),
            Event::Start(Tag::Strikethrough) => text.push_str("<s>"),
            Event::End(TagEnd::Strikethrough) => text.push_str("</s>"),
            Event::Start(Tag::Link { dest_url, .. }) => {
                // Pango's own link markup, so the label handles activation.
                text.push_str(&format!("<a href=\"{}\">", escape(&dest_url)));
            }
            Event::End(TagEnd::Link) => text.push_str("</a>"),
            Event::Start(Tag::Heading { .. }) => text.push_str("<b><big>"),
            Event::End(TagEnd::Heading(_)) => {
                text.push_str("</big></b>\n");
            }
            Event::Start(Tag::List(_)) => {
                // A nested list opens inside its parent's item, whose text has
                // just been written and has no line break after it yet.
                if list_depth > 0 {
                    text.push('\n');
                }
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    text.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                text.push_str(&"    ".repeat(list_depth.saturating_sub(1)));
                text.push_str("• ");
            }
            Event::End(TagEnd::Item) => text.push('\n'),
            Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => {
                quote_depth = quote_depth.saturating_sub(1);
            }
            Event::Start(Tag::Paragraph) => {
                if quote_depth > 0 {
                    text.push_str("<i>");
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if quote_depth > 0 {
                    text.push_str("</i>");
                }
                text.push('\n');
            }
            // Tables have no Pango equivalent, so they are laid out as text:
            // cells separated, rows on their own lines. Losing the grid is
            // better than losing the content, which is what dropping the tags
            // did — every cell ran together into one line.
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => text.push_str("  │  "),
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                // Trim the separator the last cell just added.
                while text.ends_with([' ', '│']) {
                    text.pop();
                }
                text.push('\n');
            }
            Event::SoftBreak | Event::HardBreak => text.push('\n'),
            Event::Rule => text.push_str("\n──────\n"),
            // Tables, footnotes, HTML: rendered as their own text rather than
            // dropped, since dropping loses what someone wrote.
            Event::Html(t) | Event::InlineHtml(t) => text.push_str(&escape(&t)),
            _ => {}
        }
    }
    flush(&mut text, &mut blocks);
    blocks
}

/// Escapes for Pango, then substitutes the things Mattermost writes as text
/// but means as something else: `:shortcode:` emoji and `@name` mentions.
///
/// Done after escaping, because both replacements *emit* markup and would
/// otherwise be escaped along with everything else.
fn inline(text: &str) -> String {
    let escaped = escape(text);
    let mut out = String::with_capacity(escaped.len());
    let mut rest = escaped.as_str();

    while let Some(start) = rest.find([':', '@']) {
        out.push_str(&rest[..start]);
        let sigil = rest.as_bytes()[start];
        let after = &rest[start + 1..];

        let end = match sigil {
            b':' => after.find(':').filter(|end| {
                // A shortcode has no spaces in it; "10:30 and" is not one.
                let name = &after[..*end];
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-')
            }),
            _ => Some(
                after
                    .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_')))
                    .unwrap_or(after.len()),
            ),
        };

        match (sigil, end) {
            (b':', Some(end)) => {
                let name = &after[..end];
                match crate::emoji::resolve(name) {
                    crate::emoji::Rendered::Unicode(glyph) => out.push_str(glyph),
                    // A custom emoji has no glyph to substitute, so the
                    // shortcode stays — it is at least readable.
                    crate::emoji::Rendered::Custom => {
                        out.push(':');
                        out.push_str(name);
                        out.push(':');
                    }
                }
                rest = &after[end + 1..];
            }
            (b'@', Some(end)) if end > 0 => {
                // Tinted, not linked: a mention is a highlight, and making it
                // clickable would promise a profile card this does not have.
                out.push_str("<span foreground=\"#3584e4\">@");
                out.push_str(&after[..end]);
                out.push_str("</span>");
                rest = &after[end..];
            }
            _ => {
                out.push(sigil as char);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Escapes the five characters Pango treats as markup.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(message: &str) -> String {
        match parse(message).as_slice() {
            [Block::Text(t)] => t.clone(),
            other => panic!("expected one text block, got {other:?}"),
        }
    }

    #[test]
    fn inline_styles_become_pango_spans() {
        assert_eq!(
            text_of("**bold** and *soft*"),
            "<b>bold</b> and <i>soft</i>"
        );
        assert_eq!(text_of("~~gone~~"), "<s>gone</s>");
    }

    #[test]
    fn markup_in_a_message_is_never_markup() {
        // The whole reason to escape: this is someone's text, not a tag.
        assert_eq!(
            text_of("<span foreground='red'>hi</span> & <b>"),
            "&lt;span foreground=&apos;red&apos;&gt;hi&lt;/span&gt; &amp; &lt;b&gt;"
        );
    }

    #[test]
    fn code_fences_come_out_as_their_own_block() {
        let blocks = parse("before\n\n```rust\nlet x = 1;\n```\n\nafter");
        assert_eq!(
            blocks,
            vec![
                Block::Text("before".into()),
                Block::Code {
                    language: "rust".into(),
                    text: "let x = 1;".into()
                },
                Block::Text("after".into()),
            ]
        );
    }

    #[test]
    fn code_content_is_not_escaped_twice() {
        // It goes into a non-markup label, so it must stay literal.
        let blocks = parse("```\n<b>&\n```");
        assert_eq!(
            blocks,
            vec![Block::Code {
                language: String::new(),
                text: "<b>&".into()
            }]
        );
    }

    #[test]
    fn links_keep_their_target() {
        assert_eq!(
            text_of("[docs](https://example.com/a?b=1&c=2)"),
            "<a href=\"https://example.com/a?b=1&amp;c=2\">docs</a>"
        );
    }

    #[test]
    fn lists_are_indented_by_depth() {
        assert_eq!(text_of("- one\n- two"), "• one\n• two");
        assert_eq!(text_of("- one\n    - deep"), "• one\n    • deep");
    }

    #[test]
    fn shortcodes_become_emoji_and_mentions_are_tinted() {
        assert_eq!(text_of("nice :tada:"), "nice 🎉");
        assert!(text_of("hi @anna").contains("<span foreground=\"#3584e4\">@anna</span>"));
    }

    #[test]
    fn colons_that_are_not_shortcodes_are_left_alone() {
        // A time, a ratio and an unknown name must all survive as written.
        assert_eq!(text_of("at 10:30 sharp"), "at 10:30 sharp");
        assert_eq!(text_of("ratio 4:3"), "ratio 4:3");
        assert_eq!(text_of(":not_an_emoji:"), ":not_an_emoji:");
    }

    #[test]
    fn a_table_keeps_its_cells_on_their_rows() {
        let rendered = text_of("| a | b |\n| - | - |\n| 1 | 2 |");
        assert!(rendered.contains("a  │  b"), "got {rendered:?}");
        assert!(rendered.contains("1  │  2"), "got {rendered:?}");
    }

    #[test]
    fn plain_text_survives_unchanged() {
        assert_eq!(text_of("just a sentence"), "just a sentence");
    }
}
