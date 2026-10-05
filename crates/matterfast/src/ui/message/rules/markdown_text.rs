use crate::state::AppState;

/// One system-message line as Markdown: prose that may contain `@handle`
/// mentions, turned into clickable links exactly like an ordinary message —
/// but without the sigil that only existed to trigger that machinery, so the
/// line reads "Anna joined the channel", not "@Anna joined the channel".
pub fn system_markdown(text: &str, st: &AppState) -> String {
    let display = st.teammate_name_display().to_string();
    crate::markdown::prepare_full(
        text,
        &|handle| {
            st.users
                .values()
                .find(|u| u.username == handle)
                .map(|u| u.display_name(&display))
                .filter(|name| !name.is_empty())
        },
        &|name| st.custom_emoji.contains(name),
        crate::markdown::Sigil::Drop,
    )
}

/// An ordinary message as Markdown.
///
/// A mention is only linked when it names somebody we have actually seen —
/// usernames, groups, and the special ones the server resolves for everyone.
pub fn message_markdown(text: &str, st: &AppState) -> String {
    let display = st.teammate_name_display().to_string();
    let prepared = crate::markdown::prepare_full(
        text,
        &|handle| {
            // The special ones address everybody and have no account behind
            // them, so they stay as written.
            if matches!(handle, "here" | "channel" | "all") {
                return Some(handle.to_string());
            }
            let name = st
                .users
                .values()
                .find(|u| u.username == handle)
                .map(|u| u.display_name(&display))
                .filter(|name| !name.is_empty());
            // A group is shown by the handle it is mentioned with.
            if name.is_none() && st.group(handle).is_some() {
                return Some(handle.to_string());
            }
            // Possibly somebody real who has simply never posted where we
            // were looking. Noted, so the session can find out.
            if name.is_none() {
                st.unknown_handles.borrow_mut().insert(handle.to_string());
            }
            name
        },
        &|name| st.custom_emoji.contains(name),
        crate::markdown::Sigil::Keep,
    );
    crate::ui::message::mark_mine(prepared, &st.me.username)
}

#[cfg(test)]
mod tests {
    use super::system_markdown;
    use crate::ui::message::rules::mention_sigils::with_mention_sigils;
    use crate::ui::message::rules::test_support::{post_with, state_with};

    #[test]
    fn a_join_message_reads_as_the_display_name_with_no_stray_at_and_a_working_link() {
        let post = post_with(
            "system_join_channel",
            "sin joined the channel.",
            &[("username", "sin")],
        );
        // The exact pipeline a click has to travel to become a profile card,
        // so the test exercises it rather than the string plumbing around it.
        let st = state_with("sin", "Семён", "Фомченко");
        assert_eq!(
            system_markdown(&with_mention_sigils(&post), &st),
            "[Семён Фомченко](mm-mention:sin) joined the channel."
        );
    }
}
