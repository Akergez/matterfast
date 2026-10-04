use mattermost_api::models::Post;

use super::replace_word::replace_word;

/// Adds the `@` sigil to a bare handle so `markdown::inline`'s mention
/// scanner — which only ever looks for `@handle` — notices it. The server
/// writes some system messages with a bare handle ("sin joined the
/// channel") and others already `@`-prefixed; either way the names involved
/// live in `props` under `username`, `addedUsername` and `removedUsername`,
/// which is what gets marked up here rather than scanning the sentence for
/// anything that looks like a name — the wording is localised, so
/// pattern-matching it would work in English and nowhere else.
///
/// The display-name swap and the click-through are left to the same
/// machinery an ordinary mention uses (the `known` callback and
/// `follow_link`), rather than being done here as plain text: a plain-text
/// swap never produces the `mm-mention:` link, so the result reads correctly
/// but nothing happens when you click it.
pub(crate) fn with_mention_sigils(post: &Post) -> String {
    let mut text = post.message.clone();
    for (key, value) in &post.props {
        if !key.to_lowercase().ends_with("username") {
            continue;
        }
        let Some(handle) = value.as_str().filter(|h| !h.is_empty()) else {
            continue;
        };
        if !text.contains(&format!("@{handle}")) {
            text = replace_word(&text, handle, &format!("@{handle}"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::with_mention_sigils;
    use crate::ui::message::rules::test_support::post_with;

    #[test]
    fn a_bare_handle_gets_a_sigil_so_it_becomes_a_mention() {
        let post = post_with(
            "system_join_channel",
            "sin joined the channel.",
            &[("username", "sin")],
        );
        assert_eq!(with_mention_sigils(&post), "@sin joined the channel.");
    }

    #[test]
    fn a_handle_already_written_with_a_sigil_is_left_alone() {
        // Would otherwise double up into "@@sin".
        let post = post_with(
            "system_add_to_channel",
            "@sin added to the channel by @admin",
            &[("addedUsername", "sin"), ("username", "admin")],
        );
        assert_eq!(
            with_mention_sigils(&post),
            "@sin added to the channel by @admin"
        );
    }
}
