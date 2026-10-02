//! Post, post metadata and post-list models (`server/public/model/post*.go`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{dialog::PostActionOptions, file::FileInfo, Millis};

/// Free-form `props` bag.
pub type Props = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Post {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    /// Non-zero once the post has been edited. Do **not** use `update_at` for
    /// this — it also bumps on reactions and pins.
    #[serde(default)]
    pub edit_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub channel_id: String,
    /// `""` for a root post, otherwise the root post's id.
    #[serde(default)]
    pub root_id: String,
    #[serde(default)]
    pub original_id: String,
    #[serde(default)]
    pub message: String,
    /// The user's raw text when the server rewrote `message` for presentation
    /// (image proxy). Prefer this when populating an edit box.
    #[serde(default)]
    pub message_source: String,
    /// `""` for a normal post; `system_*` for system messages.
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub props: Props,
    #[serde(default)]
    pub hashtags: String,
    /// Go writes a nil slice as `null`, and a post read back out of the
    /// search has one here where a post in a channel has `[]`.
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub file_ids: Vec<String>,
    #[serde(default)]
    pub pending_post_id: String,
    #[serde(default)]
    pub has_reactions: bool,
    #[serde(default)]
    pub reply_count: i64,
    /// Who has replied, sent with the root post itself under collapsed reply
    /// threads. Transient — the server fills it in on the way out and it is
    /// never stored — which is why it arrives before the thread is fetched.
    ///
    /// Explicitly `null` on a post with no replies, so a plain default is not
    /// enough to decode it.
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub participants: Vec<Participant>,
    #[serde(default)]
    pub last_reply_at: Millis,
    #[serde(default)]
    pub is_following: Option<bool>,
    #[serde(default)]
    pub metadata: Option<PostMetadata>,
}

impl Post {
    pub fn is_reply(&self) -> bool {
        !self.root_id.is_empty()
    }

    pub fn is_deleted(&self) -> bool {
        self.delete_at != 0
    }

    pub fn is_edited(&self) -> bool {
        self.edit_at != 0
    }

    /// System messages (joins, leaves, header changes …) are rendered
    /// differently and are excluded from unread counts.
    pub fn is_system(&self) -> bool {
        self.r#type.starts_with("system_")
    }

    /// `/me` messages.
    pub fn is_emote(&self) -> bool {
        self.r#type == "me"
    }

    /// The id a thread groups under: the root id for replies, own id otherwise.
    pub fn thread_root(&self) -> &str {
        if self.root_id.is_empty() {
            &self.id
        } else {
            &self.root_id
        }
    }

    /// Text to edit — `message_source` when the server rewrote the message.
    pub fn source_text(&self) -> &str {
        if self.message_source.is_empty() {
            &self.message
        } else {
            &self.message_source
        }
    }

    /// True for a post the client created optimistically and the server has
    /// not echoed back yet: we set the id to the pending id so the echo can
    /// replace it in place.
    pub fn is_pending(&self) -> bool {
        !self.pending_post_id.is_empty() && self.pending_post_id == self.id
    }

    pub fn from_webhook(&self) -> bool {
        self.props.get("from_webhook").and_then(|v| v.as_str()) == Some("true")
    }

    /// Webhooks and bots may override the displayed username.
    pub fn override_username(&self) -> Option<&str> {
        self.props.get("override_username").and_then(|v| v.as_str())
    }

    pub fn priority(&self) -> Option<&PostPriority> {
        self.metadata.as_ref()?.priority.as_ref()
    }

    /// The rich cards a webhook, plugin or integration attached. They live in
    /// `props`, not in metadata, and a message that carries them usually has
    /// an empty `message` — so a client that ignores them renders nothing at
    /// all for a whole class of messages.
    pub fn attachments(&self) -> Vec<MessageAttachment> {
        self.props
            .get("attachments")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    /// Link previews the server resolved for this message.
    pub fn embeds(&self) -> &[PostEmbed] {
        self.metadata
            .as_ref()
            .map(|m| m.embeds.as_slice())
            .unwrap_or(&[])
    }

    pub fn files(&self) -> &[FileInfo] {
        self.metadata
            .as_ref()
            .map(|m| m.files.as_slice())
            .unwrap_or(&[])
    }

    pub fn reactions(&self) -> &[Reaction] {
        self.metadata
            .as_ref()
            .map(|m| m.reactions.as_slice())
            .unwrap_or(&[])
    }
}

/// A rich card attached to a post (`model.SlackAttachment`). Named for its
/// Slack ancestry, which is also why the field names are what they are.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MessageAttachment {
    #[serde(default)]
    pub fallback: String,
    /// A CSS colour for the stripe down the side, when the sender set one.
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub pretext: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub title_link: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub fields: Vec<AttachmentField>,
    #[serde(default)]
    pub footer: String,
    #[serde(default)]
    pub image_url: String,
    /// Buttons and menus under the card. Go marshals a nil slice as `null`,
    /// which is what a card without any arrives with.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub actions: Vec<AttachmentAction>,
}

/// Something to press on a card (`model.PostAction`): a button, or a menu to
/// pick one value from.
///
/// The integration's own URL and context are not here. The server strips them
/// before a client sees the post and keeps them to itself; pressing the thing
/// is `POST /posts/{id}/actions/{action_id}`, and the server does the calling.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttachmentAction {
    #[serde(default)]
    pub id: String,
    /// `"button"` or `"select"`. Empty means a button, which is what the
    /// oldest integrations send.
    #[serde(default)]
    pub r#type: String,
    /// What it says on it.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub disabled: bool,
    /// `default`, `primary`, `success`, `good`, `warning`, `danger`, or a hex
    /// colour.
    #[serde(default)]
    pub style: String,
    /// For a menu: `"users"` or `"channels"` when the choices are the
    /// server's own directory rather than `options`.
    #[serde(default)]
    pub data_source: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub options: Vec<PostActionOptions>,
    /// The `value` of the option a menu starts on.
    #[serde(default)]
    pub default_option: String,
    /// Opaque, and only set on ephemeral posts, which the server does not
    /// store: it is the action's own definition, sealed, to be handed back.
    #[serde(default)]
    pub cookie: String,
}

impl AttachmentAction {
    pub fn is_select(&self) -> bool {
        self.r#type == "select"
    }
}

fn null_as_empty<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttachmentField {
    #[serde(default)]
    pub title: String,
    /// Free-form: a string, a number, or occasionally a nested object.
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub short: bool,
}

impl AttachmentField {
    /// The value as something printable, whatever shape it arrived in.
    pub fn text(&self) -> String {
        match &self.value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostMetadata {
    // Every list here may arrive as `null` rather than be left out: which it
    // is depends on the route the post came by, not on the post.
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub embeds: Vec<PostEmbed>,
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub emojis: Vec<Emoji>,
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub files: Vec<FileInfo>,
    #[serde(default, deserialize_with = "super::null_as_default")]
    pub images: HashMap<String, PostImage>,
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub priority: Option<PostPriority>,
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub acknowledgements: Vec<PostAcknowledgement>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostEmbed {
    /// `image` | `message_attachment` | `opengraph` | `link` | `permalink` |
    /// `boards`
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct PostImage {
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub frame_count: i32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostPriority {
    /// `""` | `"important"` | `"urgent"`
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub requested_ack: Option<bool>,
    #[serde(default)]
    pub persistent_notifications: Option<bool>,
}

impl PostPriority {
    pub fn is_urgent(&self) -> bool {
        self.priority.as_deref() == Some("urgent")
    }
    pub fn is_important(&self) -> bool {
        self.priority.as_deref() == Some("important")
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostAcknowledgement {
    pub user_id: String,
    pub post_id: String,
    #[serde(default)]
    pub acknowledged_at: Millis,
    #[serde(default)]
    pub channel_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Emoji {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub creator_id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Reaction {
    pub user_id: String,
    pub post_id: String,
    pub emoji_name: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub channel_id: String,
}

/// The shape every feed endpoint returns.
///
/// `order` is **newest-first**; `posts` is keyed by id.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostList {
    #[serde(default, deserialize_with = "super::null_as_empty")]
    pub order: Vec<String>,
    #[serde(default, deserialize_with = "super::null_as_default")]
    pub posts: HashMap<String, Post>,
    #[serde(default)]
    pub next_post_id: String,
    #[serde(default)]
    pub prev_post_id: String,
    #[serde(default)]
    pub has_next: Option<bool>,
    #[serde(default)]
    pub first_inaccessible_post_time: Millis,
}

impl PostList {
    /// Wraps a bare list of posts, newest first, in the envelope the rest of
    /// the client expects. `POST /posts/ids` answers with an array rather than
    /// a list, and every reader here goes through `order`.
    pub fn from_posts(mut posts: Vec<Post>) -> PostList {
        posts.sort_by_key(|p| std::cmp::Reverse(p.create_at));
        PostList {
            order: posts.iter().map(|p| p.id.clone()).collect(),
            posts: posts.into_iter().map(|p| (p.id.clone(), p)).collect(),
            ..Default::default()
        }
    }

    /// `next_post_id == ""` means this page reaches the newest post in the
    /// channel — the block is `recent` in webapp terms.
    pub fn is_recent(&self) -> bool {
        self.next_post_id.is_empty()
    }

    /// `prev_post_id == ""` means this page reaches the beginning of the
    /// channel — the block is `oldest`.
    pub fn is_oldest(&self) -> bool {
        self.prev_post_id.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Posts in `order` (newest first), skipping ids the map does not carry.
    pub fn ordered(&self) -> impl Iterator<Item = &Post> {
        self.order.iter().filter_map(|id| self.posts.get(id))
    }

    /// Oldest-first, which is the order a chat feed renders in.
    pub fn chronological(&self) -> Vec<&Post> {
        let mut v: Vec<&Post> = self.ordered().collect();
        v.reverse();
        v
    }

    pub fn newest(&self) -> Option<&Post> {
        self.order.first().and_then(|id| self.posts.get(id))
    }

    pub fn oldest_post(&self) -> Option<&Post> {
        self.order.last().and_then(|id| self.posts.get(id))
    }
}

/// One entry of the collapsed-reply-threads inbox
/// (`GET /users/{me}/teams/{team}/threads`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserThread {
    /// The root post's id — a thread is identified by its root.
    pub id: String,
    #[serde(default)]
    pub reply_count: i64,
    #[serde(default)]
    pub last_reply_at: Millis,
    #[serde(default)]
    pub last_viewed_at: Millis,
    #[serde(default)]
    pub unread_replies: i64,
    #[serde(default)]
    pub unread_mentions: i64,
    #[serde(default)]
    pub is_following: bool,
    #[serde(default)]
    pub post: Post,
    #[serde(default)]
    pub participants: Vec<Participant>,
}

impl UserThread {
    pub fn is_unread(&self) -> bool {
        self.unread_replies > 0 || self.unread_mentions > 0
    }
}

/// Thread participants come back as full users on some servers and as bare
/// `{id}` stubs on others, so accept either.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Participant {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
}

/// `GET /users/{me}/teams/{team}/threads`
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserThreads {
    #[serde(default)]
    pub threads: Vec<UserThread>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub total_unread_threads: i64,
    #[serde(default)]
    pub total_unread_mentions: i64,
}

/// `POST /teams/{team}/posts/search`
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostSearchResults {
    #[serde(flatten)]
    pub posts: PostList,
    /// Which words of each post matched, by post id. Only a server with a
    /// search engine behind it fills this in. One searching its own database
    /// has nothing to say, and says it in either of two ways: the whole map
    /// is `null`, or every hit is in it with `null` for its words. Go writes
    /// a nil map and a nil slice alike.
    #[serde(default, deserialize_with = "matches_or_nothing")]
    pub matches: HashMap<String, Vec<String>>,
}

fn matches_or_nothing<'de, D>(d: D) -> Result<HashMap<String, Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let found: Option<HashMap<String, Option<Vec<String>>>> = Option::deserialize(d)?;
    Ok(found
        .unwrap_or_default()
        .into_iter()
        .map(|(post, words)| (post, words.unwrap_or_default()))
        .collect())
}

/// `server/public/model/scheduled_post.go` — a [`Post`] plus the time to send
/// it. The server stores it separately and only creates the real post at
/// `scheduled_at`.
///
/// On the wire the Go type embeds `model.Draft`, not `model.Post`, so what
/// arrives is a strict subset of the fields below: `id`, `create_at`,
/// `update_at`, `delete_at`, `user_id`, `channel_id`, `root_id`, `message`,
/// `type`, `props`, `file_ids`, `metadata`. The rest of [`Post`] stays at its
/// default — in particular `reply_count` and `is_pinned` mean nothing here.
/// Draft also carries a top-level `priority` object where a post keeps it under
/// `metadata`, so [`Post::priority`] reads `None` on one of these.
///
/// `error_code` is filled in when a send *later* failed (`channel_archived`,
/// `no_channel_permission`, `unknown`): the scheduled post stays in the list
/// with the error on it rather than disappearing, so the client can show why.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScheduledPost {
    /// The post to send — same wire fields, flattened into this object.
    #[serde(flatten)]
    pub post: Post,
    #[serde(default)]
    pub scheduled_at: Millis,
    #[serde(default)]
    pub error_code: String,
}

/// `GET /posts/scheduled/team/{team_id}` — the scheduled posts of one team,
/// bucketed.
///
/// The body is a flat object of `bucket -> [ScheduledPost]`, built by hand in
/// the handler rather than by a named Go type: `response[teamId]` always, plus
/// `response["directChannels"]` when the request carried
/// `?includeDirectChannels=true` (`server/channels/api4/scheduled_post.go:147`).
/// Nothing else is ever put in it, and the team bucket is present even when it
/// is empty — the app layer turns a nil slice into `[]` before it gets here
/// (`server/channels/app/scheduled_post.go:65`).
///
/// So the shape does not vary and no `untagged` union is needed. Checked
/// against server 11.10 source, `Client4.GetUserScheduledPosts` (which declares
/// exactly `map[string][]*ScheduledPost`) and the published spec for the route,
/// whose minimum server version is 10.3 — the release that added it.
///
/// Kept as the map it is, because the keys are the two named above and a
/// caller that iterates them as if they were all team ids would count DMs as a
/// team. Reach for the buckets by name.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct TeamScheduledPosts(pub HashMap<String, Vec<ScheduledPost>>);

impl TeamScheduledPosts {
    /// The one key in the response that is not a team id.
    pub const DIRECT_CHANNELS: &'static str = "directChannels";

    pub fn for_team(&self, team_id: &str) -> &[ScheduledPost] {
        self.0.get(team_id).map_or(&[], Vec::as_slice)
    }

    /// Only ever populated when the request asked for it.
    pub fn direct_channels(&self) -> &[ScheduledPost] {
        self.0.get(Self::DIRECT_CHANNELS).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod null_tests {
    use super::Post;

    #[test]
    fn a_card_keeps_its_buttons_and_menus() {
        // As the server sends them: the integration's URL already stripped,
        // `options` null on a button, and no `type` at all on an old one.
        let post: Post = serde_json::from_str(
            r#"{"id":"p1","props":{"attachments":[{"text":"Deploy?","actions":[
                {"id":"ok","type":"button","name":"Approve","style":"success","options":null},
                {"id":"old","name":"Legacy"},
                {"id":"when","type":"select","name":"When","default_option":"b",
                 "options":[{"text":"A","value":"a"},{"text":"B","value":"b"}]},
                {"id":"who","type":"select","name":"Who","data_source":"users"}
            ]},{"text":"plain","actions":null}]}}"#,
        )
        .unwrap();
        let cards = post.attachments();
        assert_eq!(cards.len(), 2);
        let actions = &cards[0].actions;
        assert_eq!(actions.len(), 4);
        assert!(!actions[0].is_select() && !actions[1].is_select());
        assert!(actions[2].is_select());
        assert_eq!(actions[2].options[1].value, "b");
        assert_eq!(actions[3].data_source, "users");
        assert!(cards[1].actions.is_empty());
    }

    #[test]
    fn a_post_with_no_replies_decodes() {
        // Exactly what the server sends for a post nobody has replied to:
        // the field is present and null, not absent.
        let post: Post = serde_json::from_str(
            r#"{"id":"p1","message":"hi","participants":null,"reply_count":0}"#,
        )
        .expect("null participants must decode");
        assert!(post.participants.is_empty());

        let with: Post =
            serde_json::from_str(r#"{"id":"p1","participants":[{"id":"u1","username":"anna"}]}"#)
                .unwrap();
        assert_eq!(with.participants.len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a server searching its own database answers: `Matches` is a nil
    /// map there, and Go writes a nil map as `null`.
    #[test]
    fn search_results_without_highlights_parse() {
        let body = r#"{
            "order": ["hxr1zmb3xtdppmsfcqbdyfhz9r"],
            "posts": {"hxr1zmb3xtdppmsfcqbdyfhz9r": {
                "id": "hxr1zmb3xtdppmsfcqbdyfhz9r", "create_at": 1700000000000,
                "user_id": "kzjm6dkpppfj7cw1j5pnfjqxxy",
                "channel_id": "5b3zgc9dabnj9mymkecux3q7wc", "message": "found",
                "file_ids": null, "participants": null,
                "metadata": {"embeds": null, "emojis": null, "files": null,
                             "images": null, "reactions": null,
                             "acknowledgements": null}}},
            "next_post_id": "", "prev_post_id": "", "has_next": false,
            "first_inaccessible_post_time": 0,
            "matches": null
        }"#;
        let results: PostSearchResults = serde_json::from_str(body).unwrap();
        assert_eq!(results.posts.order.len(), 1);
        assert!(results.matches.is_empty());
    }

    /// What a real server was seen to answer: the map is there, with every
    /// hit in it, and nothing for any of them — `null` where the list of
    /// matched words would be. It is the last field of the body, which is
    /// where the decoder gave up.
    #[test]
    fn search_results_with_a_null_list_of_matches_parse() {
        let body = r#"{
            "order": ["hxr1zmb3xtdppmsfcqbdyfhz9r"],
            "posts": {"hxr1zmb3xtdppmsfcqbdyfhz9r": {
                "id": "hxr1zmb3xtdppmsfcqbdyfhz9r", "message": "found"}},
            "matches": {"hxr1zmb3xtdppmsfcqbdyfhz9r": null,
                        "kzjm6dkpppfj7cw1j5pnfjqxxy": ["found"]}
        }"#;
        let results: PostSearchResults = serde_json::from_str(body).unwrap();
        assert_eq!(results.posts.order.len(), 1);
        assert!(results.matches["hxr1zmb3xtdppmsfcqbdyfhz9r"].is_empty());
        assert_eq!(results.matches["kzjm6dkpppfj7cw1j5pnfjqxxy"], ["found"]);
    }

    /// A response body with both buckets, field for field as the Go structs
    /// tag them (`model.ScheduledPost` embedding `model.Draft`).
    #[test]
    fn a_team_scheduled_post_response_parses() {
        let body = r#"{
            "4bfx3k1jstyupfmtwmxbwaqrwh": [
                {
                    "id": "hxr1zmb3xtdppmsfcqbdyfhz9r",
                    "create_at": 1700000000000,
                    "update_at": 1700000000000,
                    "delete_at": 0,
                    "user_id": "kzjm6dkpppfj7cw1j5pnfjqxxy",
                    "channel_id": "5b3zgc9dabnj9mymkecux3q7wc",
                    "root_id": "",
                    "message": "morning",
                    "type": "",
                    "props": {},
                    "file_ids": ["ycq4kh8y8jd8xrdpmmb7hyyaqe"],
                    "metadata": {"files": [{"id": "ycq4kh8y8jd8xrdpmmb7hyyaqe", "name": "plan.pdf"}]},
                    "priority": {"priority": "urgent", "requested_ack": true},
                    "scheduled_at": 1800000000000,
                    "processed_at": 0,
                    "error_code": ""
                },
                {
                    "id": "z7uwqf1fjigt5cqozcqrfarbzh",
                    "create_at": 1700000001000,
                    "update_at": 1700000001000,
                    "delete_at": 0,
                    "user_id": "kzjm6dkpppfj7cw1j5pnfjqxxy",
                    "channel_id": "j4d7yezcsjnhipp9zebmqfsr9h",
                    "root_id": "",
                    "message": "this one failed",
                    "type": "",
                    "props": {},
                    "scheduled_at": 1800000001000,
                    "processed_at": 1800000002000,
                    "error_code": "channel_archived"
                }
            ],
            "directChannels": [
                {
                    "id": "mkbdcstm93fsjfhmpsjxjtwrmw",
                    "create_at": 1700000002000,
                    "update_at": 1700000002000,
                    "delete_at": 0,
                    "user_id": "kzjm6dkpppfj7cw1j5pnfjqxxy",
                    "channel_id": "wgdmwnnrbtn19jd4hhkgmnypia",
                    "root_id": "atqagxu5wpn6zrrww8ttrx1n1c",
                    "message": "see you then",
                    "type": "",
                    "props": {},
                    "scheduled_at": 1800000003000,
                    "processed_at": 0,
                    "error_code": ""
                }
            ]
        }"#;
        let team = "4bfx3k1jstyupfmtwmxbwaqrwh";
        let got: TeamScheduledPosts = serde_json::from_str(body).unwrap();

        let scheduled = got.for_team(team);
        assert_eq!(scheduled.len(), 2);
        assert_eq!(scheduled[0].post.id, "hxr1zmb3xtdppmsfcqbdyfhz9r");
        assert_eq!(scheduled[0].post.message, "morning");
        assert_eq!(scheduled[0].scheduled_at, 1800000000000);
        assert_eq!(scheduled[0].post.files().len(), 1);
        assert_eq!(scheduled[1].error_code, "channel_archived");

        // The bucket that is not a team.
        let dms = got.direct_channels();
        assert_eq!(dms.len(), 1);
        assert!(dms[0].post.is_reply());
        assert_eq!(got.for_team("no such team").len(), 0);

        // Without `includeDirectChannels` the key is simply absent, and the
        // team's own bucket is `[]` rather than missing when it has nothing.
        let empty: TeamScheduledPosts =
            serde_json::from_str(&format!("{{\"{team}\": []}}")).unwrap();
        assert!(empty.for_team(team).is_empty());
        assert!(empty.direct_channels().is_empty());
    }
}
