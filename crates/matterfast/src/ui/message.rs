//! One message row, shared by the channel feed and the thread panel.
//!
//! Both views draw the same thing with small differences (a thread never shows
//! a "N replies" footer, because you are already in the thread), so the
//! rendering lives here and the differences are parameters.
//!
//! The first half of this file is the part with rules in it — who reacted, how
//! a run of joins reads, how big a picture is drawn — and none of it knows
//! there is a window. The second half draws.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::text::{
    markdown_ast, InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode,
    MarkdownParseContext, MarkdownPlugin, TextView,
};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, img, px, AnyElement, App, ClickEvent, ElementId, FontWeight, ObjectFit, SharedString,
    Window,
};
use mattermost_api::models::{Millis, Post};

use super::kit::{self, Lucide};
use super::Ui;
use crate::emoji;
use crate::state::AppState;
use crate::timefmt::{format_time, has_expired};

/// Messages from the same author within this window are drawn as one group,
/// without repeating the avatar and name.
pub const GROUPING_WINDOW_MS: Millis = 5 * 60 * 1000;

/// The overflow menu's entries. One enum rather than one callback each: they
/// all travel the same path to the action loop, and the row does not care what
/// any of them mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostAction {
    Edit,
    Delete,
    Pin,
    Unpin,
    Save,
    Unsave,
    MarkUnread,
    CopyLink,
    CopyText,
    /// Ask the LLM agent to summarise this thread.
    Summarise,
    /// Send this message on to another channel.
    Forward,
    /// Have the server DM you about this later.
    Remind,
    /// Show what this message said before it was edited.
    History,
    /// Move this thread to another channel.
    MoveThread,
    /// Confirm you have read a priority message, or take it back.
    Acknowledge,
    Unacknowledge,
}

#[derive(Clone, Copy)]
pub struct RowOptions {
    /// Continuation of the previous author's group: no avatar, no name.
    pub grouped: bool,
    /// Offer the "N replies" footer. False inside a thread.
    pub show_thread_footer: bool,
    /// Briefly marked out, because something just navigated to it.
    pub highlight: bool,
}

/// Who reacted with one emoji: names in the order they reacted, with the
/// current user pulled to the front as "You" — the same shape the webapp
/// builds for its own reaction tooltip (`reaction_tooltip/index.ts`'s
/// `getNamesOfUsers`) — plus how many more reacted whose profile we do not
/// have loaded, so a reactor we cannot name still counts instead of quietly
/// vanishing from the total.
fn reaction_names(
    reactions: &[&mattermost_api::models::Reaction],
    st: &AppState,
) -> (Vec<String>, usize) {
    let mut ordered: Vec<&mattermost_api::models::Reaction> = reactions.to_vec();
    ordered.sort_by_key(|r| r.create_at);

    let display = st.teammate_name_display().to_string();
    let mut you_reacted = false;
    let mut names = Vec::new();
    let mut unresolved = 0;
    for reaction in ordered {
        if reaction.user_id == st.me.id {
            you_reacted = true;
            continue;
        }
        match st.users.get(&reaction.user_id) {
            Some(user) => names.push(user.display_name(&display)),
            None => unresolved += 1,
        }
    }
    if you_reacted {
        names.insert(0, "You".to_string());
    }
    (names, unresolved)
}

/// "Anna, Bob and you reacted with :thumbsup:" — the web's tooltip wording
/// (`reaction_tooltip.tsx`'s `tooltipTitle`), collapsed the way it collapses:
/// everyone we can name, then a trailing count for the rest once there are
/// more reactors than we have names for.
fn reaction_tooltip(names: &[String], unresolved: usize, emoji_name: &str) -> String {
    let who = match unresolved {
        0 => match names {
            [] => String::new(),
            [only] => only.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        },
        n if names.is_empty() => format!("{n} {}", plural(n as i64, "user", "users")),
        n => format!(
            "{} and {n} other {}",
            names.join(", "),
            plural(n as i64, "user", "users")
        ),
    };
    format!("{who} reacted with :{emoji_name}:")
}

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
fn with_mention_sigils(post: &Post) -> String {
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
    crate::markdown::prepare_full(
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
    )
}

/// The channel/team activity types the webapp combines client-side when
/// several happen back to back (`combineUserActivityPosts` in
/// `post_list.ts`): joins, leaves, adds and removes. A header change, a
/// rename or anything else stays its own row — there is nothing to list for
/// those.
fn is_combinable_system(post: &Post) -> bool {
    matches!(
        post.r#type.as_str(),
        "system_join_channel"
            | "system_leave_channel"
            | "system_add_to_channel"
            | "system_remove_from_channel"
            | "system_join_team"
            | "system_leave_team"
            | "system_add_to_team"
            | "system_remove_from_team"
    )
}

/// "@anna", "@anna and @bob", or "@anna and 3 others" past the small
/// limit — the same threshold `combined_system_message`'s `LastUsers`
/// collapses at (two named, then a count).
fn mention_list(handles: &[String]) -> String {
    match handles {
        [] => String::new(),
        [a] => format!("@{a}"),
        [a, b] => format!("@{a} and @{b}"),
        [a, rest @ ..] => format!("@{a} and {} others", rest.len()),
    }
}

/// One combined line per (post type, actor) pair found in the run — "Anna
/// and 3 others joined the channel." — mirroring `CombinedSystemMessage`'s
/// `postTypeMessage` table, minus the "you"/expand-in-place wrinkles: this is
/// a chat row, not a redux-connected React tree.
fn system_sentence(post_type: &str, handles: &[String], actor: Option<&str>) -> String {
    let who = mention_list(handles);
    let were = if handles.len() == 1 { "was" } else { "were" };
    match post_type {
        "system_join_channel" => format!("{who} joined the channel."),
        "system_leave_channel" => format!("{who} left the channel."),
        "system_join_team" => format!("{who} joined the team."),
        "system_leave_team" => format!("{who} left the team."),
        "system_add_to_channel" => match actor {
            Some(a) => format!("{who} {were} added to the channel by @{a}."),
            None => format!("{who} {were} added to the channel."),
        },
        "system_add_to_team" => match actor {
            Some(a) => format!("{who} {were} added to the team by @{a}."),
            None => format!("{who} {were} added to the team."),
        },
        "system_remove_from_channel" => format!("{who} {were} removed from the channel."),
        "system_remove_from_team" => format!("{who} {were} removed from the team."),
        _ => who,
    }
}

/// The prop key naming who the event happened to, and — where the event has
/// one — who did it. `remove_*` messages name no actor in the webapp either
/// (`props.username` on those posts identifies the admin's session, not
/// someone worth crediting in the sentence).
fn system_subject_and_actor(post_type: &str) -> (&'static str, Option<&'static str>) {
    match post_type {
        "system_add_to_channel" | "system_add_to_team" => ("addedUsername", Some("username")),
        "system_remove_from_channel" | "system_remove_from_team" => ("removedUsername", None),
        _ => ("username", None),
    }
}

/// One line per run of combinable system posts, per distinct (type, actor)
/// pair within it — so "Anna joined" and "Bob was added by Carol" arriving
/// back to back become two lines in one block rather than one line each with
/// its own avatar-width margin.
fn combined_system_text(posts: &[&Post]) -> String {
    let mut groups: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
    for post in posts {
        let (subject_key, actor_key) = system_subject_and_actor(&post.r#type);
        let Some(handle) = post
            .props
            .get(subject_key)
            .and_then(|v| v.as_str())
            .filter(|h| !h.is_empty())
        else {
            continue;
        };
        let actor = actor_key
            .and_then(|k| post.props.get(k))
            .and_then(|v| v.as_str())
            .filter(|a| !a.is_empty())
            .map(str::to_string);

        match groups
            .iter_mut()
            .find(|(t, a, _)| *t == post.r#type && *a == actor)
        {
            Some((_, _, handles)) => {
                if !handles.iter().any(|h| h == handle) {
                    handles.push(handle.to_string());
                }
            }
            None => groups.push((post.r#type.clone(), actor, vec![handle.to_string()])),
        }
    }

    groups
        .into_iter()
        .map(|(post_type, actor, handles)| system_sentence(&post_type, &handles, actor.as_deref()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The lines a run of consecutive system posts is drawn as: the combinable
/// ones (joins, leaves, adds, removes) collapse into one line per run,
/// everything else keeps its own. `posts` is expected to hold no more than one
/// such run interleaved with non-combinable posts — exactly what the feed
/// buffers between the messages that are not system posts at all.
pub fn system_lines(posts: &[Post], st: &AppState) -> Vec<String> {
    let mut lines = Vec::new();
    let mut run: Vec<&Post> = Vec::new();
    for post in posts {
        if is_combinable_system(post) {
            run.push(post);
            continue;
        }
        if !run.is_empty() {
            lines.push(system_markdown(&combined_system_text(&run), st));
            run.clear();
        }
        lines.push(system_markdown(&with_mention_sigils(post), st));
    }
    if !run.is_empty() {
        lines.push(system_markdown(&combined_system_text(&run), st));
    }
    lines
}

pub fn plural(n: i64, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}

/// Mattermost timestamps are Unix **milliseconds**.
/// Replaces whole-word occurrences only, so a handle that happens to be a
/// substring of another word is left alone.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        let before_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after = &rest[at + word.len()..];
        let after_ok = after
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');

        out.push_str(&rest[..at]);
        if before_ok && after_ok {
            out.push_str(with);
        } else {
            out.push_str(word);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The status as a person would read it: what it says, and until when.
///
/// The expiry is most of the information — "on holiday" matters differently
/// depending on whether they are back this afternoon or next week — and it is
/// what the other clients show alongside it.
pub fn custom_status_tooltip(status: &mattermost_api::models::CustomStatus) -> String {
    let text = if status.text.is_empty() {
        format!(":{}:", status.emoji)
    } else {
        status.text.clone()
    };
    match status
        .expires_at
        .as_deref()
        .and_then(crate::timefmt::expiry_phrase)
    {
        Some(until) => format!("{text}\n{until}"),
        None => text,
    }
}

/// Whether a custom status is worth drawing: it says something, and it has
/// not run out.
pub fn status_is_live(status: &mattermost_api::models::CustomStatus) -> bool {
    if status.emoji.is_empty() && status.text.is_empty() {
        return false;
    }
    !status
        .expires_at
        .as_deref()
        .is_some_and(|expiry| !expiry.is_empty() && has_expired(expiry))
}

/// The size to draw an attached image at: its own proportions, fitted inside
/// a box big enough to see and small enough to scroll past.
///
/// A file with no dimensions — some servers omit them — gets the full box and
/// `Contain` sorts it out once the picture arrives.
pub fn scaled_size(width: i32, height: i32) -> (i32, i32) {
    const MAX_WIDTH: f64 = 500.0;
    const MAX_HEIGHT: f64 = 350.0;

    if width <= 0 || height <= 0 {
        return (MAX_WIDTH as i32, MAX_HEIGHT as i32);
    }
    let scale = (MAX_WIDTH / width as f64)
        .min(MAX_HEIGHT / height as f64)
        // Never enlarge: a 64px sticker blown up to 420 is a blurry sticker.
        .min(1.0);
    (
        ((width as f64 * scale).round() as i32).max(1),
        ((height as f64 * scale).round() as i32).max(1),
    )
}

/// Server-side sizes available for an attached image, short of downloading
/// the original: a 120×100 thumbnail and a preview capped at 1920px wide
/// (`imageThumbnailWidth` / `imagePreviewWidth` in the Mattermost server,
/// `server/channels/app/file.go`). The thumbnail is what this app used to
/// draw every inline image at, which is why they came out blurry — it is
/// smaller than the box they were shown in even at 1x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    Thumbnail,
    Preview,
}

/// The physical box a 120×100 thumbnail is generated into, whatever the
/// source photo's own proportions.
const THUMBNAIL_WIDTH: i32 = 120;
const THUMBNAIL_HEIGHT: i32 = 100;

/// Which server-generated size to fetch for an inline image.
///
/// `scale_factor` is the window's, rounded up: on a 2x display, the
/// logical box the image is drawn into (see [`scaled_size`]) needs roughly
/// twice the source pixels to look sharp, so it is the *physical* size that
/// decides whether the thumbnail is still big enough — not the logical one.
pub fn image_size(width: i32, height: i32, scale_factor: i32, has_preview: bool) -> ImageSize {
    if !has_preview {
        // No preview was generated for this file (the server makes one for
        // almost everything, but not, say, a format it does not decode) —
        // the thumbnail is the only size short of the original, and
        // fetching the original for every such image would defeat the point
        // of a thumbnail at all.
        return ImageSize::Thumbnail;
    }
    let (logical_w, logical_h) = scaled_size(width, height);
    let scale = scale_factor.max(1);
    if logical_w.saturating_mul(scale) <= THUMBNAIL_WIDTH
        && logical_h.saturating_mul(scale) <= THUMBNAIL_HEIGHT
    {
        ImageSize::Thumbnail
    } else {
        ImageSize::Preview
    }
}

/// What a clicked link means: a person, a message on this server, or an
/// ordinary web page that the browser should have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A mention, by handle — that is all the text carries.
    Profile(String),
    /// A link to another message on this server: go there rather than to a
    /// browser, which would open the whole app again to show something
    /// already on screen.
    Permalink(String),
    Web(String),
}

pub fn link_target(url: &str) -> LinkTarget {
    if let Some(handle) = url.strip_prefix(crate::markdown::MENTION_SCHEME) {
        return LinkTarget::Profile(handle.to_string());
    }
    match permalink(url) {
        Some(post_id) => LinkTarget::Permalink(post_id),
        None => LinkTarget::Web(url.to_string()),
    }
}

/// The post id in a Mattermost permalink, if that is what this is.
///
/// The shape is `<site>/<team>/pl/<post id>`, and the team part is sometimes
/// `_redirect` — which is why the match is on the `/pl/` segment rather than
/// on the whole URL.
fn permalink(url: &str) -> Option<String> {
    let (_, tail) = url.split_once("/pl/")?;
    let id = tail.split(['/', '?', '#']).next()?;
    // Mattermost ids are 26 characters of lowercase alphanumerics; anything
    // else is a different site that happens to have /pl/ in its path.
    (id.len() == 26 && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

// ------------------------------------------------------------------ drawing

/// How many repliers to show before the count speaks for itself.
const THREAD_FACES: usize = 5;

/// A stable id for something inside a row. Ids nest under the row's own, so
/// these only have to be unique within one message.
fn eid(name: impl Into<SharedString>) -> ElementId {
    ElementId::Name(name.into())
}

/// A clicked link in a message body.
///
/// The text view insists on a handler it can share between threads, so this
/// cannot capture the session; it looks it up instead.
fn link_clicked(url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App) {
    let Some(ui) = super::current(cx) else { return };
    let url = url.to_string();
    ui.later(cx, move |ui, cx| ui.follow_link(&url, cx));
}

/// Markdown as an element. `id` has to be stable across redraws: the view
/// keeps its parsed text, and a selection, under it.
pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> TextView {
    // Built once and cloned: a clone keeps its revision, which is what lets
    // a view keep its parsed text from one frame to the next.
    static EXTENSIONS: std::sync::OnceLock<MarkdownExtensions> = std::sync::OnceLock::new();
    let extensions = EXTENSIONS.get_or_init(|| MarkdownExtensions::default().plugin(CustomEmoji).plugin(Mention));
    TextView::markdown(id, source)
        .on_link_click(link_clicked)
        .markdown_extensions(extensions.clone())
}

/// Draws the server's own emoji inside a line of text.
///
/// [`crate::markdown::prepare_full`] writes one as an image addressed by
/// name; this claims those images, so they are drawn from the picture cache
/// — which holds the session — instead of being fetched as a URL.
struct CustomEmoji;

/// A custom emoji's name, carried from parsing to drawing.
struct EmojiName(String);

impl MarkdownPlugin for CustomEmoji {
    fn name(&self) -> &str {
        "custom-emoji"
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        _: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Image(image) = node else {
            return None;
        };
        let name = image.url.strip_prefix(crate::markdown::EMOJI_SCHEME)?;
        // Copying a message gives the shortcode back, as it was typed.
        let shortcode = format!(":{name}:");
        Some(
            MarkdownNode::new(self.name().to_string(), EmojiName(name.to_string()))
                .text(shortcode.clone())
                .markdown(shortcode),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let EmojiName(name) = node.data::<EmojiName>()?;
        // Not here yet, or not on this server after all: `None` leaves the
        // shortcode standing as text, and asking is what starts the fetch.
        let picture = super::current(cx)?.avatars.custom_emoji(name)?;
        // As tall as the line, which is a little taller than the letters:
        // a picture the height of an "x" is too small to make out.
        let size = context.line_height();
        // Named by where it sits in the message: an animation keeps the
        // frame it is on under its id, and one without an id never leaves
        // its first.
        let at = node.source_range().map_or(0, |range| range.start);
        Some(InlineElement::new(
            img(picture)
                .id(ElementId::Name(format!("emoji-{name}-{at}").into()))
                .size(size)
                .object_fit(ObjectFit::Contain),
        ))
    }
}

/// Draws a mention as something to point at and press, rather than as a link.
///
/// [`crate::markdown`] writes one as a link under its own scheme; this claims
/// those links. Pointing at one shows who it is, pressing it opens their
/// profile — and the ones that are about you are marked, which is the only
/// part of a long message most people are looking for.
struct Mention;

/// A mention, carried from parsing to drawing.
struct MentionOf {
    /// The username, a group's name, or one of `here`, `channel`, `all`.
    handle: String,
    /// What the message shows: the person's name as this reader has asked
    /// for names to be shown.
    label: SharedString,
}

/// What a mention of a whole audience does, for the card that explains it.
pub(super) fn audience(handle: &str) -> Option<&'static str> {
    match handle {
        "here" => Some("Notifies everyone in this channel who is online"),
        "channel" | "all" => Some("Notifies everyone in this channel"),
        _ => None,
    }
}

impl MarkdownPlugin for Mention {
    fn name(&self) -> &str {
        "mention"
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        _: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Link(link) = node else {
            return None;
        };
        let handle = link.url.strip_prefix(crate::markdown::MENTION_SCHEME)?;
        let label: SharedString = node.to_string().into();
        Some(
            MarkdownNode::new(
                self.name().to_string(),
                MentionOf {
                    handle: handle.to_string(),
                    label: label.clone(),
                },
            )
            .text(label)
            // Copying a message gives the handle back, as it was typed.
            .markdown(format!("@{handle}")),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let MentionOf { handle, label } = node.data::<MentionOf>()?;
        let ui = super::current(cx)?;
        let theme = cx.theme();
        // Yours, or everyone's and therefore yours too.
        let about_me =
            audience(handle).is_some() || ui.state.borrow().me.username == *handle;
        let (wash, ink) = if about_me {
            (theme.warning.opacity(0.28), theme.foreground)
        } else {
            (theme.info.opacity(0.14), theme.info)
        };
        let hover = if about_me {
            theme.warning.opacity(0.42)
        } else {
            theme.info.opacity(0.26)
        };

        let at = node.source_range().map_or(0, |range| range.start);
        let pill = div()
            .id(ElementId::Name(format!("mention-{handle}-{at}").into()))
            .px(px(3.))
            .rounded_sm()
            .text_size(context.font_size())
            .line_height(context.line_height())
            .font_weight(FontWeight::MEDIUM)
            .bg(wash)
            .text_color(ink)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(label.clone())
            .on_click({
                let (ui, handle) = (ui.clone(), handle.clone());
                move |_, _, cx| {
                    cx.stop_propagation();
                    let handle = handle.clone();
                    ui.later(cx, move |ui, cx| ui.show_profile_by_handle(&handle, cx));
                }
            });

        let handle = handle.clone();
        Some(InlineElement::new(
            HoverCard::new(ElementId::Name(format!("mention-card-{handle}-{at}").into()))
                .open_delay(std::time::Duration::from_millis(350))
                .trigger(pill)
                .content(move |_, _, cx| match super::current(cx) {
                    Some(ui) => super::profile::glance(&ui, &handle, cx),
                    None => div().into_any_element(),
                }),
        ))
    }
}

/// The largest GIF fetched whole without being asked for.
const INLINE_GIF_BYTES: i64 = 8 * 1024 * 1024;

/// Whether an attachment is an animation small enough to play where it sits.
fn plays_inline(file: &mattermost_api::models::FileInfo) -> bool {
    let gif = file.mime_type == "image/gif" || file.extension.eq_ignore_ascii_case("gif");
    gif && file.size > 0 && file.size <= INLINE_GIF_BYTES
}

/// One emoji, however it has to be drawn: a Unicode glyph, or a custom upload
/// as a small picture. A custom one that has not arrived shows its shortcode,
/// which is at least readable.
pub fn emoji_element(ui: &Rc<Ui>, name: &str, size: f32) -> AnyElement {
    match emoji::resolve(name) {
        emoji::Rendered::Unicode(glyph) => div().child(glyph).into_any_element(),
        emoji::Rendered::Custom => match ui.avatars.custom_emoji(name) {
            // The id is what lets an animated one play; see `CustomEmoji`.
            Some(picture) => img(picture)
                .id(ElementId::Name(format!("emoji-{name}").into()))
                .size(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div().child(format!(":{name}:")).into_any_element(),
        },
    }
}

/// Builds a message row.
///
/// `body` is the message as Markdown, prepared when the list was built rather
/// than here: this runs for every visible row on every frame.
pub fn row(
    ui: &Rc<Ui>,
    post: &Post,
    body: &SharedString,
    options: RowOptions,
    cx: &mut App,
) -> AnyElement {
    let st = ui.state.borrow();
    let author_name = st.author_name(post);
    let author_id = post.user_id.clone();
    let presence = st.presence(&author_id);
    let custom_status = st
        .users
        .get(&author_id)
        .and_then(|u| u.custom_status())
        .filter(status_is_live);
    let mine = post.user_id == st.me.id;
    let saved = st.saved_posts.contains(&post.id);

    let theme = cx.theme();
    let muted = theme.muted_foreground;

    let mut content = v_flex().flex_1().min_w_0().gap_0p5();

    if !options.grouped {
        let mut meta = h_flex().gap_1p5().items_center().child(
            div()
                .id("author")
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .hover(|style| style.underline())
                .child(author_name.clone())
                .on_click(ui.click({
                    let author_id = author_id.clone();
                    move |ui, cx| ui.show_profile(&author_id, cx)
                })),
        );

        // Somebody's custom status — the palm tree, the house — next to their
        // name, which is where it answers the question it exists to answer:
        // are they actually around.
        if let Some(status) = &custom_status {
            meta = meta.child(kit::with_tooltip(
                "status",
                emoji_element(ui, &status.emoji, 16.),
                custom_status_tooltip(status),
            ));
        }

        meta = meta.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format_time(post.create_at)),
        );

        if let Some(priority) = post.priority() {
            if priority.is_urgent() {
                meta = meta.child(kit::tag("URGENT", theme.danger, theme.danger_foreground));
            } else if priority.is_important() {
                meta = meta.child(kit::tag("IMPORTANT", theme.primary, theme.primary_foreground));
            }
        }
        content = content.child(meta);
    }

    if !body.is_empty() {
        content = content.child(markdown(eid("body"), body.clone()));
    }

    if post.is_edited() {
        content = content.child(div().text_xs().text_color(muted).child("(edited)"));
    }

    // A webhook or plugin card. These usually come with an empty message, so
    // ignoring them renders nothing at all for the message.
    for (index, card) in post.attachments().iter().enumerate() {
        content = content.child(attachment_card(ui, &post.id, index, card, cx));
    }

    for (index, file) in post.files().iter().enumerate() {
        content = content.child(attachment(ui, index, file, cx));
    }

    // A link to another message renders as that message. The server resolves
    // it for us into the embed, so this is presentation only — following the
    // link by hand would be a second fetch for something already here.
    for (index, embed) in post.embeds().iter().enumerate() {
        if let Some(preview) = embed_preview(ui, index, embed, &st, cx) {
            content = content.child(preview);
        }
    }

    if post
        .priority()
        .and_then(|p| p.requested_ack)
        .unwrap_or(false)
    {
        content = content.child(acknowledgement(ui, post, &st, cx));
    }

    if let Some(strip) = reaction_strip(ui, post, &st, cx) {
        content = content.child(strip);
    }

    // Under CRT a reply never appears in the channel feed, so the only way into
    // a thread is this footer — it has to be present whenever there are replies.
    if options.show_thread_footer && post.reply_count > 0 {
        content = content.child(thread_footer(ui, post, &st, cx));
    }
    // A message with no replies gets no footer: starting a thread is the reply
    // button in the hover bar, and a permanent "Reply" under every message is
    // just noise.

    let theme = cx.theme();
    let leading: AnyElement = if options.grouped {
        // Keep the text aligned with the messages above it.
        div().w(px(40.)).flex_none().into_any_element()
    } else {
        // The avatar is the profile affordance, as it is in every Mattermost
        // client — so it has to behave like a button.
        div()
            .id("avatar")
            .flex_none()
            .cursor_pointer()
            .child(kit::avatar_with_presence(
                ui,
                &author_id,
                &author_name,
                40.,
                presence,
                cx,
            ))
            .on_click(ui.click({
                let author_id = author_id.clone();
                move |ui, cx| ui.show_profile(&author_id, cx)
            }))
            .into_any_element()
    };

    let actions = hover_actions(ui, post, options.show_thread_footer, mine, saved, cx);
    drop(st);

    h_flex()
        .id(ElementId::Name(format!("post-{}", post.id).into()))
        .group("message")
        .relative()
        .w_full()
        .items_start()
        .gap_2p5()
        .px_3()
        .py_0p5()
        .when(!options.grouped, |row| row.mt_2())
        .rounded_md()
        .hover(|style| style.bg(theme.list_hover))
        .when(options.highlight, |row| row.bg(theme.accent))
        // An unconfirmed send stays dimmed until the server echoes it back.
        .when(post.is_pending(), |row| row.opacity(0.55))
        .child(leading)
        .child(content)
        .child(
            div()
                .absolute()
                .top(px(-10.))
                .right_3()
                .invisible()
                .group_hover("message", |style| style.visible())
                .child(actions),
        )
        .into_any_element()
}

/// The "N replies" footer: who replied, then the count.
fn thread_footer(ui: &Rc<Ui>, post: &Post, st: &AppState, cx: &App) -> AnyElement {
    let label = format!(
        "{} {}",
        post.reply_count,
        plural(post.reply_count, "reply", "replies")
    );
    // Who replied, before the count: a thread is worth opening because of
    // who is in it, and the faces answer that before the number does.
    let display = st.teammate_name_display().to_string();
    // The post itself carries who replied — the server fills it in on
    // the way out under collapsed threads, so the faces are there
    // before the thread has been opened, let alone fetched. Falling
    // back to a loaded thread covers servers that leave it empty.
    let mut repliers: Vec<String> = post
        .participants
        .iter()
        .map(|p| p.id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    if repliers.is_empty() {
        if let Some(thread) = st.threads.get(post.thread_root()) {
            for reply in &thread.posts {
                if reply.id != post.id && !repliers.contains(&reply.user_id) {
                    repliers.push(reply.user_id.clone());
                }
            }
        }
    }
    // The root's author is named above the message already.
    repliers.retain(|id| id != &post.user_id);

    let mut footer = h_flex().gap_1p5().items_center().mt_0p5();
    for user_id in repliers.iter().take(THREAD_FACES) {
        let name = st
            .users
            .get(user_id)
            .map(|u| u.display_name(&display))
            .unwrap_or_default();
        footer = footer.child(kit::avatar(ui, user_id, &name, 20.));
    }

    let root = post.thread_root().to_string();
    footer
        .child(
            div()
                .id("replies")
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().link)
                .cursor_pointer()
                .hover(|style| style.underline())
                .child(label)
                .on_click(ui.click(move |ui, cx| ui.dispatch(super::Action::OpenThread(root.clone()), cx))),
        )
        .into_any_element()
}

/// The "please confirm you have read this" row on a priority message. Shown
/// as a button until you press it, then as who has.
fn acknowledgement(ui: &Rc<Ui>, post: &Post, st: &AppState, cx: &App) -> AnyElement {
    let acks = post
        .metadata
        .as_ref()
        .map(|m| m.acknowledgements.as_slice())
        .unwrap_or_default();
    let mine = acks.iter().any(|a| a.user_id == st.me.id);

    let button = Button::new("acknowledge")
        .label(if mine { "Acknowledged" } else { "Acknowledge" })
        .small()
        .when(mine, |button| button.success())
        .when(!mine, |button| button.primary())
        .on_click(ui.click({
            let post_id = post.id.clone();
            move |ui, cx| {
                ui.dispatch(
                    super::Action::Post(
                        post_id.clone(),
                        if mine {
                            PostAction::Unacknowledge
                        } else {
                            PostAction::Acknowledge
                        },
                    ),
                    cx,
                )
            }
        }));

    let mut row = h_flex().gap_2().items_center().mt_1().child(button);
    if !acks.is_empty() {
        let names: Vec<String> = acks
            .iter()
            .filter_map(|a| st.users.get(&a.user_id))
            .map(|u| st.display_name(u))
            .collect();
        row = row.child(kit::with_tooltip(
            "acknowledged",
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{} acknowledged", acks.len())),
            names.join(", "),
        ));
    }
    row.into_any_element()
}

/// A CSS-style colour — "#2eb886", or one of the three words Slack-compatible
/// integrations use — as something to paint with.
fn card_color(color: &str, cx: &App) -> Option<gpui_kit::Hsla> {
    match color {
        "good" => Some(cx.theme().success),
        "warning" => Some(cx.theme().warning),
        "danger" => Some(cx.theme().danger),
        hex => {
            let hex = hex.strip_prefix('#')?;
            if hex.len() != 6 {
                return None;
            }
            let value = u32::from_str_radix(hex, 16).ok()?;
            Some(gpui_kit::rgb(value).into())
        }
    }
}

/// Something to press on a card: a button, or a menu that sends what was
/// picked. Neither changes anything here — the integration answers by editing
/// the post, and that edit is what the person sees.
fn card_action(
    ui: &Rc<Ui>,
    post_id: &str,
    card: usize,
    action: &mattermost_api::models::AttachmentAction,
    cx: &App,
) -> AnyElement {
    let id = ElementId::Name(format!("card-{card}-action-{}", action.id).into());
    let button = Button::new(id).small().disabled(action.disabled);
    let press = {
        let (ui, post_id) = (ui.clone(), post_id.to_string());
        let (action_id, cookie) = (action.id.clone(), action.cookie.clone());
        move |selected: String, cx: &mut App| {
            ui.dispatch(
                super::Action::CardAction {
                    post_id: post_id.clone(),
                    action_id: action_id.clone(),
                    selected,
                    cookie: cookie.clone(),
                },
                cx,
            )
        }
    };

    if !action.is_select() {
        let button = button.label(action.name.clone());
        // The words are the server's. Anything else is a colour of the
        // integration's own, drawn the way the web client draws it: the
        // colour as text on a faint wash of itself, which stays readable
        // whatever was chosen.
        let button = match action.style.as_str() {
            "primary" => button.primary(),
            "success" | "good" => button.success(),
            "warning" => button.warning(),
            "danger" => button.danger(),
            other => match card_color(other, cx) {
                Some(color) => button.custom(
                    ButtonCustomVariant::new(cx)
                        .color(color.opacity(0.1))
                        .foreground(color)
                        .hover(color.opacity(0.2))
                        .active(color.opacity(0.3)),
                ),
                None => button.outline(),
            },
        };
        return button
            .on_click(move |_, _, cx| press(String::new(), cx))
            .into_any_element();
    }

    // A menu starts on its default, and otherwise on its own name, which is
    // what integrations use as the placeholder.
    let label = action
        .options
        .iter()
        .find(|option| !action.default_option.is_empty() && option.value == action.default_option)
        .map(|option| option.text.clone())
        .unwrap_or_else(|| action.name.clone());
    let button = button.label(label).outline().dropdown_caret(true);

    // The server's own people or channels: too many for a menu, so a list.
    if matches!(action.data_source.as_str(), "users" | "channels") {
        let ui = ui.clone();
        let (title, channels) = (action.name.clone(), action.data_source == "channels");
        let press = Rc::new(press);
        return button
            .on_click(move |_, _, cx| {
                let (title, press) = (title.clone(), press.clone());
                ui.later(cx, move |ui, cx| {
                    ui.pick_from_directory(&title, channels, move |value, cx| press(value, cx), cx)
                });
            })
            .into_any_element();
    }

    let options = action.options.clone();
    let current = action.default_option.clone();
    let press = Rc::new(press);
    let _ = cx;
    button
        .dropdown_menu(move |mut menu, _, _| {
            for option in &options {
                let (press, value) = (press.clone(), option.value.clone());
                menu = menu.item(
                    PopupMenuItem::new(option.text.clone())
                        .checked(!current.is_empty() && option.value == current)
                        .on_click(move |_, _, cx| press(value.clone(), cx)),
                );
            }
            menu
        })
        .into_any_element()
}

/// One rich card: a coloured stripe, a title that may be a link, some text,
/// and its fields laid out as label-and-value rows.
fn attachment_card(
    ui: &Rc<Ui>,
    post_id: &str,
    index: usize,
    card: &mattermost_api::models::MessageAttachment,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut content = v_flex().flex_1().min_w_0().gap_0p5().pl_2p5();
    let mut empty = true;

    if !card.author_name.is_empty() {
        content = content.child(div().text_xs().text_color(muted).child(card.author_name.clone()));
        empty = false;
    }
    if !card.pretext.is_empty() {
        content = content.child(div().text_color(muted).child(card.pretext.clone()));
        empty = false;
    }
    if !card.title.is_empty() {
        // A title with a link is a link; without one it is just bold.
        let title = div().font_weight(FontWeight::SEMIBOLD);
        content = content.child(if card.title_link.is_empty() {
            title.child(card.title.clone()).into_any_element()
        } else {
            let url = card.title_link.clone();
            title
                .id("title")
                .text_color(cx.theme().link)
                .cursor_pointer()
                .hover(|style| style.underline())
                .child(card.title.clone())
                .on_click(move |_, _, cx| crate::open_url(&url, cx))
                .into_any_element()
        });
        empty = false;
    }
    if !card.text.is_empty() {
        content = content.child(markdown(
            eid("text"),
            crate::markdown::prepare_with(&card.text, &|_| None, crate::markdown::Sigil::Keep),
        ));
        empty = false;
    }

    for field in &card.fields {
        let value = field.text();
        if field.title.is_empty() && value.is_empty() {
            continue;
        }
        let mut row = v_flex().mt_1();
        if !field.title.is_empty() {
            row = row.child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(field.title.clone()),
            );
        }
        if !value.is_empty() {
            row = row.child(div().child(value));
        }
        content = content.child(row);
        empty = false;
    }

    if !card.actions.is_empty() {
        let mut row = h_flex().mt_1p5().gap_1p5().flex_wrap();
        for action in card.actions.iter().filter(|action| !action.id.is_empty()) {
            row = row.child(card_action(ui, post_id, index, action, cx));
        }
        content = content.child(row);
        empty = false;
    }

    if !card.footer.is_empty() {
        content = content.child(div().text_xs().text_color(muted).child(card.footer.clone()));
        empty = false;
    }
    // Nothing usable in the card itself: the fallback is what it is for.
    if empty && !card.fallback.is_empty() {
        content = content.child(div().child(card.fallback.clone()));
    }

    // The sender's colour, where they gave one — it usually encodes the status
    // of whatever the card reports, so it carries meaning. Without one, a
    // muted version of the text colour, so it still reads as a card.
    let stripe = card_color(&card.color, cx).unwrap_or(cx.theme().foreground.opacity(0.3));
    h_flex()
        .id(("card", index))
        .w_full()
        .items_stretch()
        .mt_1()
        .child(div().flex_none().w(px(3.)).rounded_sm().bg(stripe))
        .child(content)
        .into_any_element()
}

/// The bordered card a quoted message or a link preview sits in.
fn preview_card(cx: &App) -> gpui_kit::Div {
    v_flex()
        .mt_1()
        .p_2()
        .gap_0p5()
        .max_w(px(520.))
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(0.4))
}

/// A link the server resolved into a title and a description.
fn link_preview(
    index: usize,
    embed: &mattermost_api::models::PostEmbed,
    cx: &App,
) -> Option<AnyElement> {
    let data = embed.data.as_ref()?;
    let read = |key: &str| {
        data.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let title = match read("title") {
        t if t.is_empty() => read("site_name"),
        t => t,
    };
    // Without a title there is nothing to preview that the link text does not
    // already say.
    if title.is_empty() {
        return None;
    }

    let url = if embed.url.is_empty() {
        read("url")
    } else {
        embed.url.clone()
    };
    let description = read("description");
    let mut card = preview_card(cx).child(
        div()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title),
    );
    if !description.is_empty() {
        card = card.child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .line_clamp(3)
                .child(description),
        );
    }
    Some(
        card.id(("link", index))
            .cursor_pointer()
            .hover(|style| style.border_color(cx.theme().ring))
            .on_click(move |_, _, cx| crate::open_url(&url, cx))
            .into_any_element(),
    )
}

/// The quoted message behind a permalink, as a compact card; or, for an
/// ordinary link, what the server made of it.
fn embed_preview(
    ui: &Rc<Ui>,
    index: usize,
    embed: &mattermost_api::models::PostEmbed,
    st: &AppState,
    cx: &App,
) -> Option<AnyElement> {
    if embed.r#type == "opengraph" || embed.r#type == "link" {
        return link_preview(index, embed, cx);
    }
    if embed.r#type != "permalink" {
        return None;
    }
    // The embed carries the post under a `post` key; anything else is a
    // permalink the server could not resolve, and a card saying nothing is
    // worse than the bare link already in the text.
    let quoted: Post = serde_json::from_value(
        embed
            .data
            .as_ref()?
            .get("post")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .ok()?;

    let author = st.author_name(&quoted);
    let channel = st
        .channel(&quoted.channel_id)
        .map(|c| st.channel_title(c))
        .unwrap_or_default();
    let muted = cx.theme().muted_foreground;

    let mut header = h_flex()
        .gap_1p5()
        .items_center()
        .child(kit::avatar(ui, &quoted.user_id, &author, 20.))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(author))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format_time(quoted.create_at)),
        );
    if !channel.is_empty() {
        header = header.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("in {channel}")),
        );
    }

    // Clicking it goes there, which is what the link would have done.
    let root = quoted.thread_root().to_string();
    Some(
        preview_card(cx)
            .id(("quote", index))
            .cursor_pointer()
            .hover(|style| style.border_color(cx.theme().ring))
            .child(header)
            .child(markdown(
                eid("quoted"),
                message_markdown(&quoted.message, st),
            ))
            .on_click(ui.click(move |ui, cx| {
                ui.dispatch(super::Action::OpenThread(root.clone()), cx)
            }))
            .into_any_element(),
    )
}

/// An attached file. Images show themselves; everything else is a name and a
/// size, which is all there is to say about it without opening it.
fn attachment(
    ui: &Rc<Ui>,
    index: usize,
    file: &mattermost_api::models::FileInfo,
    cx: &App,
) -> AnyElement {
    if file.is_image() {
        // Big enough to actually look at. The other clients go to roughly
        // this, and a thumbnail you have to open to see is a thumbnail that
        // makes you open everything.
        //
        // Both bounds matter: capping height alone turns a wide screenshot
        // into a strip, and capping width alone lets a tall photo run down
        // the page.
        let (width, height) = scaled_size(file.width, file.height);
        let source = image_size(
            file.width,
            file.height,
            ui.scale_factor().ceil() as i32,
            file.has_preview_image,
        );
        let still = || match source {
            ImageSize::Preview => ui.avatars.file_preview(&file.id),
            ImageSize::Thumbnail => ui.avatars.file_thumbnail(&file.id),
        };
        // A GIF is posted to be watched. The server's preview of one is a
        // still, so a small enough GIF is fetched whole and plays in place;
        // until it lands, and for the big ones, the still stands in.
        let picture = if plays_inline(file) {
            ui.avatars.file_animation(&file.id).or_else(still)
        } else {
            still()
        };
        let frame = div()
            .id(("image", index))
            .mt_1()
            .w(px(width as f32))
            .h(px(height as f32))
            .max_w_full()
            .rounded_md()
            .overflow_hidden()
            .cursor_pointer()
            .on_click(ui.click({
                let file = file.clone();
                move |ui, cx| ui.open_image(&file, cx)
            }));
        return match picture {
            Some(picture) => frame
                .child(
                    img(picture)
                        // An animation plays only where it has an id to
                        // keep its place under.
                        .id("picture")
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            // The box is the picture's own size from the start, so the
            // conversation does not jump when it lands.
            None => frame.bg(cx.theme().muted).into_any_element(),
        };
    }

    // Video and audio play in place; see `super::media`.
    if super::media::is_playable(file) {
        return super::media::player(ui, index, file, cx);
    }

    // Everything that is not an image: a name, a size, and a way to get it.
    h_flex()
        .id(("file", index))
        .mt_1()
        .gap_1p5()
        .items_center()
        .text_color(cx.theme().muted_foreground)
        .child(kit::Lucide::Paperclip)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(format!("{}  ·  {}", file.name, file.human_size())),
        )
        .child(
            kit::icon_button("save", kit::Lucide::Download, "Save").on_click(ui.click({
                let file = file.clone();
                move |ui, cx| ui.save_attachment(&file, cx)
            })),
        )
        .into_any_element()
}

/// The small react / save / reply buttons that appear over a row on hover.
fn hover_actions(
    ui: &Rc<Ui>,
    post: &Post,
    allow_thread: bool,
    mine: bool,
    saved: bool,
    cx: &App,
) -> AnyElement {
    let post_id = post.id.clone();

    // The eight most-used sit in the popover, because most reactions are one
    // of them; the full table is one more click away.
    let react = Popover::new("react")
        .trigger(kit::icon_button("react-button", Lucide::FaceSlightlySmilingPlus, "Add reaction"))
        .content({
            let ui = ui.clone();
            let post_id = post_id.clone();
            move |_, _, cx| {
                // Picking is the end of the gesture: the palette goes away
                // with the choice rather than waiting for a click elsewhere.
                let popover = cx.entity();
                let mut quick = h_flex().gap_0p5();
                for (index, name) in emoji::QUICK_REACTIONS.iter().enumerate() {
                    let name = (*name).to_string();
                    let post_id = post_id.clone();
                    quick = quick.child(
                        Button::new(("quick", index))
                            .ghost()
                            .small()
                            .label(emoji::label(&name))
                            .tooltip(format!(":{name}:"))
                            .on_click({
                                let popover = popover.clone();
                                let pick = ui.click(move |ui, cx| {
                                    ui.dispatch(
                                        super::Action::ToggleReaction(
                                            post_id.clone(),
                                            name.clone(),
                                        ),
                                        cx,
                                    )
                                });
                                move |event, window, cx| {
                                    popover.update(cx, |state, cx| state.dismiss(window, cx));
                                    pick(event, window, cx);
                                }
                            }),
                    );
                }
                let post_id = post_id.clone();
                quick
                    .child(
                        kit::icon_button("more", Lucide::Search, "Search every emoji").on_click({
                            let pick = ui.click(move |ui, cx| ui.pick_reaction(post_id.clone(), cx));
                            move |event, window, cx| {
                                popover.update(cx, |state, cx| state.dismiss(window, cx));
                                pick(event, window, cx);
                            }
                        }),
                    )
                    .into_any_element()
            }
        });

    let mut bar = h_flex()
        .gap_0p5()
        .p_0p5()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .shadow_sm()
        .child(react);

    // Saving is one click in every other client, so it is a button here too
    // rather than being buried in the menu.
    bar = bar.child(
        kit::icon_button(
            "save",
            if saved { Lucide::BookmarkCheck } else { Lucide::Bookmark },
            if saved { "Remove from saved" } else { "Save" },
        )
        .on_click(ui.click({
            let post_id = post_id.clone();
            move |ui, cx| {
                ui.dispatch(
                    super::Action::Post(
                        post_id.clone(),
                        if saved { PostAction::Unsave } else { PostAction::Save },
                    ),
                    cx,
                )
            }
        })),
    );

    if allow_thread {
        let root = post.thread_root().to_string();
        bar = bar.child(
            kit::icon_button("reply", Lucide::Reply, "Reply in thread").on_click(ui.click(
                move |ui, cx| ui.dispatch(super::Action::OpenThread(root.clone()), cx),
            )),
        );
    }

    bar.child(overflow_menu(ui, post, mine))
        .into_any_element()
}

/// What the "…" menu offers for a post. Editing and deleting are only offered
/// on your own posts — the server would refuse anyway, and an option that
/// always fails is worse than no option.
fn post_menu_entries(post: &Post, mine: bool) -> Vec<(&'static str, PostAction)> {
    let mut entries = vec![
        ("Copy text", PostAction::CopyText),
        ("Copy link", PostAction::CopyLink),
        ("Mark as unread", PostAction::MarkUnread),
        ("Forward…", PostAction::Forward),
        ("Remind me about this…", PostAction::Remind),
    ];
    if post.is_pinned {
        entries.push(("Unpin from channel", PostAction::Unpin));
    } else {
        entries.push(("Pin to channel", PostAction::Pin));
    }
    // Only worth offering where there is a history to see.
    if post.is_edited() {
        entries.push(("Edit history", PostAction::History));
    }
    if post.reply_count > 0 {
        entries.push(("Summarise thread", PostAction::Summarise));
        entries.push(("Move thread…", PostAction::MoveThread));
    }
    if mine {
        entries.push(("Edit", PostAction::Edit));
        entries.push(("Delete", PostAction::Delete));
    }
    entries
}

fn overflow_menu(ui: &Rc<Ui>, post: &Post, mine: bool) -> AnyElement {
    let entries = post_menu_entries(post, mine);
    let post_id = post.id.clone();
    let ui = ui.clone();
    kit::icon_button("more-actions", Lucide::Ellipsis, "More actions")
        .dropdown_menu(move |mut menu, _, _| {
            for (label, action) in &entries {
                let action = *action;
                let post_id = post_id.clone();
                menu = menu.item(PopupMenuItem::new(*label).on_click(ui.click(
                    move |ui, cx| ui.dispatch(super::Action::Post(post_id.clone(), action), cx),
                )));
            }
            menu
        })
        .into_any_element()
}

/// Reactions, collapsed by emoji and drawn as actual emoji rather than
/// `:shortcodes:`. Clicking a chip toggles our own reaction, as everywhere else.
fn reaction_strip(ui: &Rc<Ui>, post: &Post, st: &AppState, cx: &App) -> Option<AnyElement> {
    let reactions = post.reactions();
    if reactions.is_empty() {
        return None;
    }

    // Preserve first-seen order rather than sorting: it matches what the other
    // clients show and keeps chips from jumping around as counts change.
    let mut grouped: Vec<(String, Vec<&mattermost_api::models::Reaction>)> = Vec::new();
    for reaction in reactions {
        match grouped
            .iter_mut()
            .find(|(name, _)| *name == reaction.emoji_name)
        {
            Some((_, group)) => group.push(reaction),
            None => grouped.push((reaction.emoji_name.clone(), vec![reaction])),
        }
    }

    let theme = cx.theme();
    let mut strip = h_flex().flex_wrap().gap_1().mt_1();
    for (index, (name, group)) in grouped.into_iter().enumerate() {
        let mine = group.iter().any(|r| r.user_id == st.me.id);
        let (names, unresolved) = reaction_names(&group, st);
        let tooltip: SharedString = reaction_tooltip(&names, unresolved, &name).into();
        let post_id = post.id.clone();
        let toggle = name.clone();
        strip = strip.child(
            h_flex()
                .id(("reaction", index))
                .gap_1()
                .px_1p5()
                .h(px(24.))
                .items_center()
                .rounded_full()
                .border_1()
                .text_sm()
                .cursor_pointer()
                .border_color(if mine { theme.primary } else { theme.border })
                .bg(if mine {
                    theme.primary.opacity(0.12)
                } else {
                    theme.muted.opacity(0.5)
                })
                .hover(|style| style.border_color(theme.ring))
                .child(emoji_element(ui, &name, 16.))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .child(group.len().to_string()),
                )
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_click(ui.click(move |ui, cx| {
                    ui.dispatch(
                        super::Action::ToggleReaction(post_id.clone(), toggle.clone()),
                        cx,
                    )
                })),
        );
    }
    Some(strip.into_any_element())
}

/// A run of system lines: joins, leaves, a header change.
pub fn system_block(index_key: &str, lines: &[SharedString], cx: &App) -> AnyElement {
    let mut block = v_flex()
        .id(ElementId::Name(format!("system-{index_key}").into()))
        .w_full()
        .gap_0p5()
        // Under the avatar column, so it reads as part of the conversation.
        .pl(px(62.))
        .pr_3()
        .py_0p5()
        .text_sm()
        .text_color(cx.theme().muted_foreground);
    for (index, line) in lines.iter().enumerate() {
        block = block.child(markdown(("line", index), line.clone()));
    }
    block.into_any_element()
}

#[cfg(test)]
mod size_tests {
    use super::scaled_size;

    #[test]
    fn fits_the_box_without_enlarging() {
        // A tall photo is bounded by height, a wide one by width.
        assert_eq!(scaled_size(3000, 4000), (263, 350));
        assert_eq!(scaled_size(4000, 1000), (500, 125));
        // Smaller than the box: left alone.
        assert_eq!(scaled_size(64, 64), (64, 64));
        // Unknown: the full box, and Contain sorts it out.
        assert_eq!(scaled_size(0, 0), (500, 350));
    }
}

#[cfg(test)]
mod image_source_tests {
    use super::{image_size, ImageSize};

    #[test]
    fn a_small_sticker_stays_on_the_thumbnail_at_1x() {
        // 64x64 fits inside the 120x100 thumbnail box with room to spare.
        assert_eq!(image_size(64, 64, 1, true), ImageSize::Thumbnail);
    }

    #[test]
    fn the_same_sticker_needs_the_preview_at_2x() {
        // 64x64 doubled is 128x128, past the thumbnail's own 120x100 box.
        assert_eq!(image_size(64, 64, 2, true), ImageSize::Preview);
    }

    #[test]
    fn an_ordinary_photo_always_wants_the_preview() {
        // Clamped to 467x350 by `scaled_size`, already past the thumbnail.
        assert_eq!(image_size(1600, 1200, 1, true), ImageSize::Preview);
    }

    #[test]
    fn no_preview_on_the_server_falls_back_to_the_thumbnail_regardless_of_size() {
        assert_eq!(image_size(1600, 1200, 2, false), ImageSize::Thumbnail);
    }
}

#[cfg(test)]
mod permalink_tests {
    use super::permalink;

    #[test]
    fn recognises_a_message_link() {
        assert_eq!(
            permalink("https://mm.example.com/team/pl/gedfji9g1pbjjehsngn9j17fzr").as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );
        // The double slash a reminder message produces, and _redirect.
        assert_eq!(
            permalink("https://mm.example.com//pl/gedfji9g1pbjjehsngn9j17fzr").as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );
        assert_eq!(
            permalink("https://mm.example.com/_redirect/pl/gedfji9g1pbjjehsngn9j17fzr?x=1")
                .as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );

        // Not ours.
        assert_eq!(permalink("https://example.com/pl/short"), None);
        assert_eq!(permalink("https://example.com/blog/post"), None);
    }
}

#[cfg(test)]
mod reaction_tooltip_tests {
    use super::reaction_tooltip;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn one_named_reactor() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna"]), 0, "thumbsup"),
            "Anna reacted with :thumbsup:"
        );
    }

    #[test]
    fn two_named_reactors_get_an_and_not_a_comma() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna", "You"]), 0, "thumbsup"),
            "Anna and You reacted with :thumbsup:"
        );
    }

    #[test]
    fn three_or_more_are_comma_joined_before_the_last_and() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna", "Bob", "You"]), 0, "tada"),
            "Anna, Bob and You reacted with :tada:"
        );
    }

    #[test]
    fn reactors_with_no_loaded_profile_become_a_trailing_count() {
        assert_eq!(
            reaction_tooltip(&names(&["Anna"]), 3, "fire"),
            "Anna and 3 other users reacted with :fire:"
        );
        // Singular agreement, and no named reactors at all.
        assert_eq!(
            reaction_tooltip(&[], 1, "fire"),
            "1 user reacted with :fire:"
        );
    }
}


#[cfg(test)]
mod word_tests {
    use super::replace_word;

    #[test]
    fn replaces_whole_words_only() {
        assert_eq!(replace_word("sin joined", "sin", "Anna"), "Anna joined");
        // A handle inside another word is not that person.
        assert_eq!(
            replace_word("sinner joined", "sin", "Anna"),
            "sinner joined"
        );
        assert_eq!(replace_word("ask sin.", "sin", "Anna"), "ask Anna.");
        assert_eq!(replace_word("sin_bot left", "sin", "Anna"), "sin_bot left");
    }
}


#[cfg(test)]
mod system_message_tests {
    use super::*;

    fn post_with(post_type: &str, message: &str, props: &[(&str, &str)]) -> Post {
        Post {
            r#type: post_type.to_string(),
            message: message.to_string(),
            props: props
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect(),
            ..Default::default()
        }
    }

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

    fn state_with(username: &str, first: &str, last: &str) -> AppState {
        let client = mattermost_api::Client::new("http://x.test").unwrap();
        let mut st = AppState::new(
            client,
            Default::default(),
            mattermost_api::models::ClientConfig::default(),
            false,
        );
        st.config
            .0
            .insert("TeammateNameDisplay".into(), "full_name".into());
        st.users.insert(
            "u1".into(),
            mattermost_api::models::User {
                id: "u1".into(),
                username: username.into(),
                first_name: first.into(),
                last_name: last.into(),
                ..Default::default()
            },
        );
        st
    }

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

    #[test]
    fn a_link_is_a_person_a_message_or_a_web_page() {
        assert_eq!(link_target("mm-mention:anna"), LinkTarget::Profile("anna".into()));
        assert_eq!(
            link_target("https://mm.example.com/team/pl/gedfji9g1pbjjehsngn9j17fzr"),
            LinkTarget::Permalink("gedfji9g1pbjjehsngn9j17fzr".into())
        );
        assert_eq!(
            link_target("https://example.com/blog/post"),
            LinkTarget::Web("https://example.com/blog/post".into())
        );
    }

    #[test]
    fn a_run_of_system_posts_becomes_one_line_per_kind() {
        let st = state_with("anna", "Anna", "Petrova");
        let posts = vec![
            post_with("system_join_channel", "", &[("username", "anna")]),
            post_with("system_join_channel", "", &[("username", "bob")]),
            post_with("system_header_change", "anna changed the header", &[("username", "anna")]),
        ];
        assert_eq!(
            system_lines(&posts, &st),
            vec![
                // Known people by name, unknown ones as written.
                "[Anna Petrova](mm-mention:anna) and @bob joined the channel.".to_string(),
                "[Anna Petrova](mm-mention:anna) changed the header".to_string(),
            ]
        );
    }

    #[test]
    fn combinable_types_are_exactly_the_ones_the_webapp_merges() {
        for combinable in [
            "system_join_channel",
            "system_leave_channel",
            "system_add_to_channel",
            "system_remove_from_channel",
            "system_join_team",
            "system_leave_team",
            "system_add_to_team",
            "system_remove_from_team",
        ] {
            assert!(is_combinable_system(&post_with(combinable, "", &[])));
        }
        // A header change has nobody to list, so it stays its own row.
        assert!(!is_combinable_system(&post_with(
            "system_header_change",
            "",
            &[]
        )));
    }

    #[test]
    fn mention_list_collapses_past_two_names() {
        let names = |n: usize| (0..n).map(|i| format!("u{i}")).collect::<Vec<_>>();
        assert_eq!(mention_list(&names(1)), "@u0");
        assert_eq!(mention_list(&names(2)), "@u0 and @u1");
        assert_eq!(mention_list(&names(3)), "@u0 and 2 others");
        assert_eq!(mention_list(&names(5)), "@u0 and 4 others");
    }

    #[test]
    fn system_sentence_agrees_singular_and_plural() {
        assert_eq!(
            system_sentence("system_join_channel", &["a".into()], None),
            "@a joined the channel."
        );
        assert_eq!(
            system_sentence(
                "system_add_to_channel",
                &["a".into(), "b".into()],
                Some("admin")
            ),
            "@a and @b were added to the channel by @admin."
        );
        assert_eq!(
            system_sentence("system_remove_from_team", &["a".into()], None),
            "@a was removed from the team."
        );
    }

    #[test]
    fn a_run_combines_into_one_line_per_type_and_actor() {
        // Two joins and an add-by-someone-else, back to back — three system
        // posts, but only the joins belong on the same line.
        let joined_a = post_with("system_join_channel", "", &[("username", "anna")]);
        let joined_b = post_with("system_join_channel", "", &[("username", "bob")]);
        let added = post_with(
            "system_add_to_channel",
            "",
            &[("addedUsername", "carol"), ("username", "dave")],
        );
        let run = [&joined_a, &joined_b, &added];
        assert_eq!(
            combined_system_text(&run),
            "@anna and @bob joined the channel.\n@carol was added to the channel by @dave."
        );
    }

    #[test]
    fn the_same_person_named_twice_in_a_run_is_not_duplicated() {
        let a = post_with("system_join_channel", "", &[("username", "anna")]);
        let b = post_with("system_join_channel", "", &[("username", "anna")]);
        let run = [&a, &b];
        assert_eq!(combined_system_text(&run), "@anna joined the channel.");
    }
}
