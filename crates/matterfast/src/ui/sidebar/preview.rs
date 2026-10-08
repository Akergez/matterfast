use mattermost_api::models::{Channel, Post};

use crate::state::AppState;

/// The longest a preview is made: a row shows a line of it at most, and a
/// message can be a page.
const LONGEST: usize = 160;

/// What a row says under a conversation's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Preview {
    /// Something of the reader's own, waiting to be sent.
    Draft(String),
    /// The last message, and who wrote it where that needs saying.
    Last { author: Option<String>, text: String },
    /// A message that was the last one once: the conversation has been
    /// written in since, and what was written is not here yet. Said, so that
    /// an old line is not read as the news.
    Stale { author: Option<String>, text: String },
    /// Nothing of this conversation is held.
    Nothing,
}

/// One line out of a message: its first line that says anything, without the
/// runs of space a wrapped paragraph has, cut to a length a row could show.
pub(super) fn one_line(message: &str) -> String {
    let line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let mut said = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((cut, _)) = said.char_indices().nth(LONGEST) {
        said.truncate(cut);
        said.push('…');
    }
    said
}

/// What there is to say of `post` in one line: its text, or that it is a
/// file where it has none.
fn said_in(post: &Post) -> String {
    let text = one_line(&post.message);
    if !text.is_empty() {
        return text;
    }
    match post.files().len() {
        0 => String::new(),
        1 => "Attachment".to_string(),
        files => format!("{files} attachments"),
    }
}

/// The preview for a conversation, out of what the state holds of it.
pub(super) fn preview(channel: &Channel, st: &AppState) -> Preview {
    if let Some(draft) = st.drafts.get(&channel.id).map(|draft| one_line(draft)) {
        if !draft.is_empty() {
            return Preview::Draft(draft);
        }
    }
    let Some(post) = st.feeds.get(&channel.id).and_then(|feed| feed.posts.last()) else {
        return Preview::Nothing;
    };
    // In a conversation of two the other person needs no naming, and a
    // system line names whoever it is about.
    let direct = channel.dm_teammate_id(&st.me.id).is_some();
    let author = if post.user_id == st.me.id {
        Some("You".to_string())
    } else if direct || post.is_system() {
        None
    } else {
        Some(st.author_name(post)).filter(|name| !name.is_empty())
    };
    let text = said_in(post);
    // A card from an integration, say: nothing to put on a line, and a name
    // with a colon after it says less than an empty line does.
    if text.is_empty() {
        return Preview::Nothing;
    }
    // With collapsed threads a reply is written in the channel and is not
    // in its feed, so what the feed is measured against is the last post
    // that started something.
    let written = match st.crt_enabled && channel.last_root_post_at > 0 {
        true => channel.last_root_post_at,
        false => channel.last_post_at,
    };
    if written > post.create_at {
        Preview::Stale { author, text }
    } else {
        Preview::Last { author, text }
    }
}

#[cfg(test)]
mod tests {
    use super::one_line;

    #[test]
    fn a_preview_is_the_first_line_that_says_anything() {
        assert_eq!(one_line("\n\n  Standup moved.  \nSee the calendar."), "Standup moved.");
        assert_eq!(one_line("a   wrapped\tparagraph"), "a wrapped paragraph");
        assert_eq!(one_line("   \n  "), "");
    }

    #[test]
    fn a_long_line_is_cut_on_a_letter_and_says_so() {
        let long = "я".repeat(400);
        let cut = one_line(&long);
        assert_eq!(cut.chars().count(), super::LONGEST + 1);
        assert!(cut.ends_with('…'));
    }
}
