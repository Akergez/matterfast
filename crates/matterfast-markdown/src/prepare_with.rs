use crate::prepare_full::prepare_full;
use crate::sigil::Sigil;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::anna;

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
}
