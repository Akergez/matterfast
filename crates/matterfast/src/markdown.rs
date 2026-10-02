//! Mattermost messages are Markdown — almost.
//!
//! The text view renders standard Markdown, and a message is that plus three
//! things Mattermost writes as text but means as something else:
//!
//! * `:shortcode:` is an emoji,
//! * `@handle` is a person, and worth showing by name,
//! * a single newline is a line break, where Markdown reads it as a space.
//!
//! So this is not a renderer. It rewrites the message into Markdown that says
//! what was meant, and leaves the drawing to the view. Everything it does not
//! recognise passes through byte for byte — a message is arbitrary text from
//! another person, and the safest thing to do with the parts that are not ours
//! is nothing.
//!
//! The rewriting only touches prose. Code, inline or fenced, is somebody
//! *showing* a shortcode or a handle rather than using it.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// The URL scheme mentions are linked with. Not a real scheme — it exists so
/// the link handler can tell "open this person" from "open this web page".
pub const MENTION_SCHEME: &str = "mm-mention:";

/// How a mention is written into the Markdown.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    /// "@Anna Petrova" — an ordinary message.
    Keep,
    /// "Anna Petrova" — a system line, which reads "Anna joined the channel".
    Drop,
}

/// Rewrites a message, linking every mention.
///
/// Everything resolves to itself when nobody says otherwise — used by the
/// tests, which have no roster to check against.
#[cfg(test)]
pub fn prepare(message: &str) -> String {
    prepare_with(message, &|handle| Some(handle.to_string()), Sigil::Keep)
}

/// Rewrites a message, linking only the mentions `known` recognises. `known`
/// answers with the name to show for a handle, or `None` when it resolves to
/// nobody.
pub fn prepare_with(
    message: &str,
    known: &dyn Fn(&str) -> Option<String>,
    sigil: Sigil,
) -> String {
    prepare_full(message, known, &|_| false, sigil)
}

/// The scheme a custom emoji's picture is addressed by inside prepared
/// Markdown: `![:name:](mm-emoji:name)`. Nothing is fetched from it; the
/// message view recognises it and draws the picture from the image cache.
pub const EMOJI_SCHEME: &str = "mm-emoji:";

/// [`prepare_with`], and the server's own emoji become pictures. `custom`
/// says whether a shortcode is one of them — asked rather than assumed,
/// because "see a:b:c" is full of things shaped like shortcodes.
pub fn prepare_full(
    message: &str,
    known: &dyn Fn(&str) -> Option<String>,
    custom: &dyn Fn(&str) -> bool,
    sigil: Sigil,
) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let mut out = String::with_capacity(message.len() + 16);
    // How much of the source has been copied or replaced so far.
    let mut copied = 0;
    // A run of prose still waiting to be rewritten. The parser hands text over
    // in pieces — an underscore is its own event — and a shortcode such as
    // `:slightly_smiling_face:` only reads as one once they are joined again.
    let mut prose: Option<std::ops::Range<usize>> = None;
    let mut in_code = 0usize;
    let mut in_link = 0usize;

    let flush = |prose: &mut Option<std::ops::Range<usize>>,
                     out: &mut String,
                     copied: &mut usize,
                     linkable: bool| {
        let Some(range) = prose.take() else { return };
        out.push_str(&message[*copied..range.start]);
        out.push_str(&inline(&message[range.clone()], known, custom, sigil, linkable));
        *copied = range.end;
    };

    for (event, range) in Parser::new_ext(message, options).into_offset_iter() {
        match event {
            Event::Text(_) if in_code == 0 => match prose.as_mut() {
                Some(run) if run.end == range.start => run.end = range.end,
                _ => {
                    flush(&mut prose, &mut out, &mut copied, in_link == 0);
                    prose = Some(range);
                }
            },
            other => {
                flush(&mut prose, &mut out, &mut copied, in_link == 0);
                match other {
                    Event::Start(Tag::CodeBlock(_)) => in_code += 1,
                    Event::End(TagEnd::CodeBlock) => in_code = in_code.saturating_sub(1),
                    // A picture in a message is a request to somebody else's
                    // server, made on the reader's behalf the moment the
                    // message is drawn. Dropping the "!" turns it into the
                    // link it already nearly was: still there, but only
                    // followed when asked.
                    Event::Start(Tag::Image { .. }) => {
                        if message[range.start..].starts_with('!') {
                            out.push_str(&message[copied..range.start]);
                            copied = range.start + 1;
                        }
                        in_link += 1;
                    }
                    // A mention inside a link's own text cannot become a
                    // second link, so it is left as written.
                    Event::Start(Tag::Link { .. }) => in_link += 1,
                    Event::End(TagEnd::Link | TagEnd::Image) => {
                        in_link = in_link.saturating_sub(1)
                    }
                    // Two trailing spaces are how Markdown spells the line
                    // break the author thought they were typing.
                    Event::SoftBreak => {
                        out.push_str(&message[copied..range.start]);
                        out.push_str("  \n");
                        copied = range.end;
                    }
                    _ => {}
                }
            }
        }
    }
    flush(&mut prose, &mut out, &mut copied, in_link == 0);
    out.push_str(&message[copied..]);
    out
}

/// Substitutes the emoji and mentions in a run of prose.
fn inline(
    text: &str,
    known: &dyn Fn(&str) -> Option<String>,
    custom: &dyn Fn(&str) -> bool,
    sigil: Sigil,
    linkable: bool,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // Where `rest` starts in `text`, for looking back at what came before.
    let mut consumed = 0;

    while let Some(start) = rest.find([':', '@']) {
        out.push_str(&rest[..start]);
        let at = consumed + start;
        let sigil_byte = rest.as_bytes()[start];
        let after = &rest[start + 1..];

        // A URL is full of colons and can carry an `@`; none of them are ours.
        if inside_url(text, at) {
            out.push(sigil_byte as char);
            consumed = at + 1;
            rest = after;
            continue;
        }

        match sigil_byte {
            b':' => {
                let shortcode = after.find(':').map(|end| &after[..end]).filter(|name| {
                    // A shortcode has no spaces in it; "10:30 and" is not one.
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
                });
                match shortcode.map(|name| (name, crate::emoji::resolve(name))) {
                    Some((name, crate::emoji::Rendered::Unicode(glyph))) => {
                        out.push_str(glyph);
                        consumed = at + name.len() + 2;
                        rest = &after[name.len() + 1..];
                    }
                    // One of the server's own: a picture, addressed by name.
                    Some((name, _)) if custom(name) => {
                        out.push_str("![:");
                        out.push_str(name);
                        out.push_str(":](");
                        out.push_str(EMOJI_SCHEME);
                        out.push_str(name);
                        out.push(')');
                        consumed = at + name.len() + 2;
                        rest = &after[name.len() + 1..];
                    }
                    // Not an emoji at all: it stays as written, and the
                    // closing colon is left to be looked at again —
                    // "10:30:tada:" still has one in it.
                    _ => {
                        out.push(':');
                        consumed = at + 1;
                        rest = after;
                    }
                }
            }
            _ => {
                let end = after
                    .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_')))
                    .unwrap_or(after.len());
                // The sigil only counts at the start of a word, which is what
                // keeps `a@b.com` an email address.
                let starts_word = text[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '_');
                let found = (linkable && starts_word)
                    .then(|| resolve_handle(&after[..end], known))
                    .flatten();
                match found {
                    Some((handle, shown)) => {
                        out.push('[');
                        if sigil == Sigil::Keep {
                            out.push('@');
                        }
                        out.push_str(&escape_label(&shown));
                        out.push_str("](");
                        out.push_str(MENTION_SCHEME);
                        out.push_str(handle);
                        out.push(')');
                        consumed = at + 1 + handle.len();
                        rest = &after[handle.len()..];
                    }
                    None => {
                        out.push('@');
                        consumed = at + 1;
                        rest = after;
                    }
                }
            }
        }
    }
    out.push_str(rest);
    out
}

/// The handle at the start of a candidate, and the name to show for it.
///
/// A handle may contain dots, dashes and underscores, and so may the sentence
/// around it: "ask @anna." names anna, not "anna.". The longest spelling that
/// resolves wins, so a real `first.last` handle is still found whole.
fn resolve_handle<'a>(
    candidate: &'a str,
    known: &dyn Fn(&str) -> Option<String>,
) -> Option<(&'a str, String)> {
    let mut handle = candidate;
    loop {
        if handle.is_empty() {
            return None;
        }
        if let Some(shown) = known(handle) {
            return Some((handle, shown));
        }
        // Only trailing punctuation can be the sentence's rather than the
        // handle's; one mark at a time, so "a.b." tries "a.b" before "a".
        match handle.char_indices().next_back() {
            Some((last, '.' | '-' | '_')) => handle = &handle[..last],
            _ => return None,
        }
    }
}

/// Whether the byte at `at` sits inside a word that is a URL.
fn inside_url(text: &str, at: usize) -> bool {
    // By character, not by byte: a no-break space is whitespace too, and it
    // is two bytes wide.
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(index, c)| index + c.len_utf8());
    let end = text[at..]
        .find(char::is_whitespace)
        .map_or(text.len(), |space| at + space);
    text[start..end].contains("://")
}

/// A display name as link text: brackets and backslashes are the only
/// characters that could end the label early.
fn escape_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(ch, '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// The first line of a message with its Markdown markers taken off — what a
/// one-line preview shows. Emoji are substituted; mentions stay as written,
/// because a preview has nowhere to link them to.
pub fn preview(message: &str) -> String {
    let line = message.lines().next().unwrap_or_default();
    let mut out = String::with_capacity(line.len());
    for event in Parser::new_ext(line, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    inline(&out, &|_| None, &|_| false, Sigil::Keep, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anna(handle: &str) -> Option<String> {
        (handle == "anna").then(|| "Anna Petrova".to_string())
    }

    #[test]
    fn markdown_that_is_already_markdown_is_left_alone() {
        for message in [
            "**bold** and *soft*",
            "~~gone~~",
            "[docs](https://example.com/a?b=1&c=2)",
            "- one\n- two",
            "- one\n    - deep",
            "| a | b |\n| - | - |\n| 1 | 2 |",
            "just a sentence",
            "<span foreground='red'>hi</span> & <b>",
        ] {
            assert_eq!(prepare_with(message, &|_| None, Sigil::Keep), message);
        }
    }

    #[test]
    fn code_is_shown_not_used() {
        // Inline or fenced, it is somebody quoting the shortcode and the
        // handle rather than meaning them.
        for message in [
            "`:tada: @anna`",
            "```\n:tada: @anna\n```",
            "before\n\n```rust\nlet x = \":tada:\";\n```\n\nafter",
        ] {
            assert_eq!(prepare_with(message, &anna, Sigil::Keep), message);
        }
    }

    #[test]
    fn an_unknown_mention_is_left_alone() {
        assert_eq!(
            prepare_with("hi @nobody and @anna", &anna, Sigil::Keep),
            // Shown by name, linked by handle: the label says who was
            // addressed, the target says which account to open.
            "hi @nobody and [@Anna Petrova](mm-mention:anna)"
        );
    }

    #[test]
    fn the_servers_own_emoji_become_pictures() {
        let custom = |name: &str| name == "shipit";
        let prepared = |text: &str| prepare_full(text, &|_| None, &custom, Sigil::Keep);
        assert_eq!(
            prepared("merged :shipit: :tada:"),
            "merged ![:shipit:](mm-emoji:shipit) 🎉"
        );
        // Shaped like a shortcode is not enough.
        assert_eq!(prepared("see a:zz9:c"), "see a:zz9:c");
        assert_eq!(prepared("`:shipit:`"), "`:shipit:`");
        assert_eq!(
            prepared("10:30:shipit:"),
            "10:30![:shipit:](mm-emoji:shipit)"
        );
    }

    #[test]
    fn shortcodes_become_emoji_and_mentions_are_linked() {
        assert_eq!(prepare("nice :tada:"), "nice 🎉");
        // Linked, not merely coloured: clicking a mention opens the person.
        assert_eq!(prepare("hi @anna"), "hi [@anna](mm-mention:anna)");
    }

    #[test]
    fn a_shortcode_with_underscores_is_still_one_shortcode() {
        // The parser splits text at every underscore, so this only works if
        // the pieces are put back together before they are read.
        assert_eq!(prepare("ok :slightly_smiling_face:"), "ok 🙂");
        assert_eq!(prepare(":+1:"), "👍");
    }

    #[test]
    fn colons_that_are_not_shortcodes_are_left_alone() {
        // A time, a ratio and an unknown name must all survive as written.
        assert_eq!(prepare("at 10:30 sharp"), "at 10:30 sharp");
        assert_eq!(prepare("ratio 4:3"), "ratio 4:3");
        assert_eq!(prepare(":not_an_emoji:"), ":not_an_emoji:");
        // And a real one straight after something that was not.
        assert_eq!(prepare("10:30:tada:"), "10:30🎉");
    }

    #[test]
    fn a_single_newline_is_a_line_break() {
        assert_eq!(prepare("one\ntwo"), "one  \ntwo");
        // A blank line is already a paragraph and needs no help.
        assert_eq!(prepare("one\n\ntwo"), "one\n\ntwo");
    }

    #[test]
    fn a_mention_ends_where_the_sentence_does() {
        assert_eq!(
            prepare_with("ask @anna.", &anna, Sigil::Keep),
            "ask [@Anna Petrova](mm-mention:anna)."
        );
        // A handle that really does contain a dot is found whole.
        let dotted = |handle: &str| (handle == "a.b").then(|| "A B".to_string());
        assert_eq!(
            prepare_with("ask @a.b.", &dotted, Sigil::Keep),
            "ask [@A B](mm-mention:a.b)."
        );
    }

    #[test]
    fn an_address_or_a_url_is_not_a_mention() {
        let everyone = |handle: &str| Some(handle.to_string());
        assert_eq!(
            prepare_with("mail a@anna.example", &everyone, Sigil::Keep),
            "mail a@anna.example"
        );
        assert_eq!(
            prepare_with("see https://x.test/@anna/:tada:/y", &everyone, Sigil::Keep),
            "see https://x.test/@anna/:tada:/y"
        );
    }

    #[test]
    fn a_mention_inside_a_link_stays_text() {
        assert_eq!(
            prepare_with("[ask @anna](https://x.test)", &anna, Sigil::Keep),
            "[ask @anna](https://x.test)"
        );
    }

    #[test]
    fn an_embedded_picture_becomes_a_link() {
        assert_eq!(
            prepare("look ![a cat](https://x.test/cat.png) here"),
            "look [a cat](https://x.test/cat.png) here"
        );
    }

    #[test]
    fn a_system_line_names_people_without_the_sigil() {
        assert_eq!(
            prepare_with("@anna joined the channel.", &anna, Sigil::Drop),
            "[Anna Petrova](mm-mention:anna) joined the channel."
        );
    }

    #[test]
    fn a_name_cannot_break_out_of_its_link() {
        let odd = |_: &str| Some("A [b] \\c".to_string());
        assert_eq!(
            prepare_with("@x", &odd, Sigil::Keep),
            "[@A \\[b\\] \\\\c](mm-mention:x)"
        );
    }

    #[test]
    fn a_preview_is_one_plain_line() {
        assert_eq!(preview("**done** :tada:\nsecond line"), "done 🎉");
        assert_eq!(preview("see `code` and [docs](https://x.test)"), "see code and docs");
        assert_eq!(preview(""), "");
    }
}
