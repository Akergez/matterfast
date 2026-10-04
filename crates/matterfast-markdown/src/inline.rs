use matterfast_emoji::{resolve, Rendered};

use crate::constants::{EMOJI_SCHEME, MENTION_SCHEME};
use crate::escape_label::escape_label;
use crate::inside_url::inside_url;
use crate::resolve_handle::resolve_handle;
use crate::sigil::Sigil;

/// Substitutes the emoji and mentions in a run of prose.
pub(crate) fn inline(
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
                match shortcode.map(|name| (name, resolve(name))) {
                    Some((name, Rendered::Unicode(glyph))) => {
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
