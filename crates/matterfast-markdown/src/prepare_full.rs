use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::inline::inline;
use crate::sigil::Sigil;

/// `prepare_with`, and the server's own emoji become pictures. `custom`
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
    let mut prose: Option<Range<usize>> = None;
    let mut in_code = 0usize;
    let mut in_link = 0usize;

    let flush = |prose: &mut Option<Range<usize>>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::prepare;

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
    fn an_embedded_picture_becomes_a_link() {
        assert_eq!(
            prepare("look ![a cat](https://x.test/cat.png) here"),
            "look [a cat](https://x.test/cat.png) here"
        );
    }
}
