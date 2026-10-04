use pulldown_cmark::{Event, Options, Parser};

use crate::inline::inline;
use crate::sigil::Sigil;

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

    #[test]
    fn a_preview_is_one_plain_line() {
        assert_eq!(preview("**done** :tada:\nsecond line"), "done 🎉");
        assert_eq!(preview("see `code` and [docs](https://x.test)"), "see code and docs");
        assert_eq!(preview(""), "");
    }
}
