use std::collections::HashSet;

use mattermost_api::models::{CategoryType, Channel, ChannelType, Millis, Post};

use super::inbox::{Entry, Inbox};
use super::inbox_row::InboxRow;
use super::target::Target;
use crate::state::AppState;
use crate::ui::notify::wants_everything;

fn inbox_row_for(post: &Post, st: &AppState) -> InboxRow {
    InboxRow {
        user_id: post.user_id.clone(),
        author: st.author_name(post),
        channel: st
            .channel(&post.channel_id)
            .map(|c| st.channel_title(c))
            .unwrap_or_else(|| "unknown channel".into()),
        preview: crate::markdown::preview(&post.message).into(),
        at: post.create_at,
        counts: None,
        saved: st.saved_posts.contains(&post.id),
        target: if post.is_reply() {
            Target::Thread {
                channel_id: post.channel_id.clone(),
                root_id: post.thread_root().to_string(),
            }
        } else {
            Target::Message {
                channel_id: post.channel_id.clone(),
                post_id: post.id.clone(),
            }
        },
    }
}

fn direct(channel: &Channel) -> bool {
    matches!(channel.r#type, ChannelType::Direct | ChannelType::Group)
}

/// Whether a conversation is followed as a whole, and so has a line of its
/// own in the inbox: a favourite, a channel the reader asked to hear all of,
/// or a direct conversation.
///
/// A direct conversation is how a message nobody answered reaches the inbox
/// — to the server it is not a thread until somebody does — and so it keeps
/// its line after being read: a line that went away then took the
/// conversation out from under the person reading it. Except where a thread
/// of that conversation has a line already (`threaded`): then a read
/// conversation would be there twice, once as itself and once as its thread,
/// and next to each other. With something unread in it, it is there anyway.
fn followed(
    channel: &Channel,
    favourites: &HashSet<&str>,
    threaded: &HashSet<&str>,
    st: &AppState,
) -> bool {
    if favourites.contains(channel.id.as_str()) {
        return true;
    }
    if direct(channel) {
        return !threaded.contains(channel.id.as_str()) || st.unread(&channel.id).is_unread();
    }
    wants_everything(&st.me, st.memberships.get(&channel.id))
}

/// When a conversation that has a line of its own was last written in. For
/// a channel that is when something was last started there: what was
/// answered in a thread has that thread's line to move, where the reader
/// follows it.
fn written(channel: &Channel) -> Millis {
    match direct(channel) || channel.last_root_post_at == 0 {
        true => channel.last_post_at,
        false => channel.last_root_post_at,
    }
}

/// The inbox: everything the reader is being spoken to in, or follows, in
/// one list with the newest first.
///
/// - The threads they follow, which is the ones they were named in, the ones
///   they wrote in, and every thread of a direct conversation: the server
///   keeps that list and it is asked for a page at a time.
/// - The messages that name them and are not in one of those threads —
///   nobody has answered yet — and the messages they saved, out of what is
///   loaded.
/// - The conversations followed as a whole ([`followed`]).
///
/// Nothing is listed twice: a mention inside a followed thread is that
/// thread's line, and a saved message that also names the reader is one
/// line that says it was saved.
pub(super) fn build_inbox(st: &AppState) -> Inbox {
    let mut found: Vec<(Millis, Entry)> = Vec::new();
    let mut threads: HashSet<&str> = HashSet::new();
    let mut posts: HashSet<&str> = HashSet::new();

    for thread in &st.thread_inbox {
        threads.insert(thread.id.as_str());
        let at = thread.last_reply_at.max(thread.post.create_at);
        let row = InboxRow {
            user_id: thread.post.user_id.clone(),
            author: st.author_name(&thread.post),
            channel: st
                .channel(&thread.post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default(),
            preview: crate::markdown::preview(&thread.post.message).into(),
            at,
            counts: Some((
                thread.reply_count,
                thread.unread_replies,
                thread.unread_mentions,
            )),
            saved: st.saved_posts.contains(&thread.id),
            target: Target::Followed {
                channel_id: thread.post.channel_id.clone(),
                root_id: thread.id.clone(),
            },
        };
        found.push((at, Entry::Post(row)));
    }

    // Saved posts are held as a set of ids, so this walks what is loaded
    // rather than fetching — anything not in memory shows up as soon as its
    // channel is opened. The same post can be held by a channel feed and by
    // its thread, which is one more reason to count ids.
    let saved = st
        .feeds
        .values()
        .chain(st.threads.values())
        .flat_map(|feed| feed.posts.iter())
        .filter(|post| st.saved_posts.contains(&post.id));
    for post in st.mentions.iter().chain(saved) {
        if threads.contains(post.thread_root()) || !posts.insert(post.id.as_str()) {
            continue;
        }
        found.push((post.create_at, Entry::Post(inbox_row_for(post, st))));
    }

    let favourites: HashSet<&str> = st
        .categories
        .categories
        .iter()
        .filter(|category| category.r#type == CategoryType::Favorites)
        .flat_map(|category| category.channel_ids.iter().map(String::as_str))
        .collect();
    // The conversations a listed thread is in.
    let threaded: HashSet<&str> = st
        .thread_inbox
        .iter()
        .map(|thread| thread.post.channel_id.as_str())
        .collect();
    for channel in st.chats() {
        if followed(channel, &favourites, &threaded, st) {
            found.push((written(channel), Entry::Chat(channel.id.clone())));
        }
    }

    // Stable, so that two things of one moment keep the order they were
    // found in: threads, then messages, then conversations.
    found.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    Inbox {
        entries: found.into_iter().map(|(_, entry)| entry).collect(),
        more: (st.thread_inbox.len() as i64) < st.thread_inbox_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::{
        ChannelMember, ClientConfig, OrderedSidebarCategories, SidebarCategory, User, UserThread,
    };
    use mattermost_api::Client;

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let me = User { id: "me".into(), username: "me".into(), ..Default::default() };
        let mut st = AppState::new(client, me, ClientConfig::default(), false);
        st.users.insert(
            "u1".into(),
            User {
                id: "u1".into(),
                username: "anna".into(),
                ..Default::default()
            },
        );
        st
    }

    fn post(id: &str, channel: &str, at: Millis) -> Post {
        Post {
            id: id.into(),
            user_id: "u1".into(),
            create_at: at,
            channel_id: channel.into(),
            message: format!("**{id}** says"),
            ..Default::default()
        }
    }

    /// What each line is, in order: a thread or message by its target's
    /// post, a conversation by its channel with a `#` before it.
    fn lines(inbox: &Inbox) -> Vec<String> {
        inbox
            .entries
            .iter()
            .map(|entry| match entry {
                Entry::Chat(id) => format!("#{id}"),
                Entry::Post(row) => match &row.target {
                    Target::Followed { root_id, .. } => format!("thread {root_id}"),
                    Target::Thread { root_id, .. } => format!("reply in {root_id}"),
                    Target::Message { post_id, .. } => format!("message {post_id}"),
                },
            })
            .collect()
    }

    #[test]
    fn a_saved_post_held_twice_is_listed_once() {
        let mut st = state();
        let saved = post("p1", "c1", 10);
        st.saved_posts.insert("p1".into());
        st.feeds.insert(
            "c1".into(),
            crate::state::ChannelFeed::from_posts(vec![saved.clone()]),
        );
        st.threads
            .insert("p1".into(), crate::state::ChannelFeed::from_posts(vec![saved]));

        let inbox = build_inbox(&st);
        assert_eq!(lines(&inbox), ["message p1"]);
        let Entry::Post(row) = &inbox.entries[0] else { panic!("a message") };
        // A preview is read, not rendered: no Markdown markers in it.
        assert_eq!(row.preview.as_ref(), "p1 says");
        assert!(row.saved);
    }

    #[test]
    fn threads_messages_and_conversations_are_one_list_with_the_newest_first() {
        let mut st = state();
        // A followed thread, last answered at 50.
        st.thread_inbox = vec![UserThread {
            id: "root".into(),
            last_reply_at: 50,
            post: post("root", "c1", 5),
            ..Default::default()
        }];
        // A mention inside that thread, which is the thread's own line; one
        // nobody has answered, which is a line; and the same one saved.
        let mut reply = post("reply", "c1", 40);
        reply.root_id = "root".into();
        st.mentions = vec![reply, post("named", "c1", 30)];
        st.saved_posts.insert("named".into());
        st.feeds.insert(
            "c1".into(),
            crate::state::ChannelFeed::from_posts(vec![post("named", "c1", 30)]),
        );
        // A favourite, written in between them.
        st.channels.insert(
            "fav".into(),
            Channel { id: "fav".into(), last_post_at: 45, ..Default::default() },
        );
        st.categories = OrderedSidebarCategories {
            order: vec!["favourites".into()],
            categories: vec![SidebarCategory {
                id: "favourites".into(),
                r#type: CategoryType::Favorites,
                channel_ids: vec!["fav".into()],
                ..Default::default()
            }],
        };

        let inbox = build_inbox(&st);
        assert_eq!(lines(&inbox), ["thread root", "#fav", "message named"]);
    }

    #[test]
    fn a_conversation_has_a_line_when_it_is_followed_as_a_whole() {
        let mut st = state();
        let mut chat = |id: &str, kind: ChannelType, read: i64, level: Option<&str>, muted: bool| {
            st.channels.insert(
                id.into(),
                Channel {
                    id: id.into(),
                    r#type: kind,
                    last_post_at: 100,
                    // Something was started here earlier than it was last
                    // answered: a channel goes by the former.
                    last_root_post_at: 60,
                    total_msg_count: 10,
                    ..Default::default()
                },
            );
            let mut member = ChannelMember {
                channel_id: id.into(),
                msg_count: read,
                ..Default::default()
            };
            if let Some(level) = level {
                member.notify_props.insert("desktop".into(), level.into());
            }
            if muted {
                member.notify_props.insert("mark_unread".into(), "mention".into());
            }
            st.memberships.insert(id.into(), member);
        };
        chat("loud", ChannelType::Open, 10, Some("all"), false);
        chat("loud-but-muted", ChannelType::Open, 10, Some("all"), true);
        chat("mentions-only", ChannelType::Open, 4, Some("mention"), false);
        chat("dm-unread", ChannelType::Direct, 9, None, false);
        chat("dm-read", ChannelType::Direct, 10, None, false);
        st.categories = OrderedSidebarCategories {
            order: vec!["channels".into()],
            categories: vec![SidebarCategory {
                id: "channels".into(),
                channel_ids: ["loud", "loud-but-muted", "mentions-only", "dm-unread", "dm-read"]
                    .map(String::from)
                    .to_vec(),
                ..Default::default()
            }],
        };

        // The direct conversations were written in at 100, read or not; the
        // channel had something started in it at 60.
        assert_eq!(lines(&build_inbox(&st)), ["#dm-unread", "#dm-read", "#loud"]);

        // A thread in each of the direct conversations has a line of its
        // own, last answered at 70. The one that was read is now there
        // once, as its thread; the one with something unread in it is
        // still there as itself as well.
        let in_thread = |channel: &str| UserThread {
            id: format!("root-{channel}"),
            last_reply_at: 70,
            post: post(&format!("root-{channel}"), channel, 5),
            ..Default::default()
        };
        st.thread_inbox = vec![in_thread("dm-unread"), in_thread("dm-read")];
        assert_eq!(
            lines(&build_inbox(&st)),
            ["#dm-unread", "thread root-dm-unread", "thread root-dm-read", "#loud"]
        );
    }
}
