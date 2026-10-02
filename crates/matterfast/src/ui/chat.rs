//! Pane 3: the conversation — header, call banner, message list, composer.
//!
//! [`ChatView`] is what the rest of the application talks to: plain state the
//! pane is drawn from, plus the two things that only exist while there is a
//! window — the composer and the scrolling list. [`render`] turns it into
//! elements, on every frame, from whatever that state says right now.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{
    Enter, Escape, IndentInline, InputEvent, MoveDown, MoveUp, Textarea, TextareaState,
};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable, Size};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, list, px, AnyElement, App, ClipboardEntry, Entity, ExternalPaths, FollowMode,
    FontWeight, ListAlignment, ListOffset, ListState, SharedString, Window,
};
use mattermost_api::models::{Millis, Post};

use super::autocomplete::{self, Candidate, Completions};
use super::kit::{self, Lucide};
use super::message::{self, RowOptions};
use super::{Action, MenuAction, Ui, WindowSlot};
use crate::state::{AppState, SharedState};
use crate::timefmt::format_day;

pub(super) fn scroll_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("MATTERFAST_SCROLL_TRACE")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    })
}

/// How far past the visible rows the list keeps things laid out, so a flick
/// of the wheel lands on rows that already exist.
const OVERDRAW: gpui_kit::Pixels = px(600.);

/// Rows from the top at which the page before this one is asked for.
const PAGINATE_WITHIN: usize = 4;

/// One row of the feed.
///
/// Built once per redraw of the conversation rather than once per frame: the
/// expensive parts — turning a message into Markdown, deciding who it groups
/// with — are done here, and drawing a row is then only laying it out.
#[derive(Clone)]
pub enum FeedItem {
    /// "This is the beginning of …".
    Start(String),
    Empty,
    Day(String),
    Unread,
    /// A run of joins, leaves and the like, as lines of Markdown.
    System {
        key: String,
        lines: Rc<Vec<SharedString>>,
    },
    Post {
        post: Rc<Post>,
        grouped: bool,
        /// The message as Markdown, ready for the text view.
        body: SharedString,
    },
}

impl FeedItem {
    pub fn post_id(&self) -> Option<&str> {
        match self {
            FeedItem::Post { post, .. } => Some(&post.id),
            _ => None,
        }
    }

    /// Which row this is, as opposed to what it currently says. Two feeds are
    /// compared by these to find what was added and what was taken away; a
    /// row whose key survives keeps its place in the list, and with it the
    /// reader's scroll position.
    fn key(&self) -> String {
        match self {
            FeedItem::Start(_) => "start".to_string(),
            FeedItem::Empty => "empty".to_string(),
            FeedItem::Day(day) => format!("day:{day}"),
            FeedItem::Unread => "unread".to_string(),
            FeedItem::System { key, .. } => format!("system:{key}"),
            FeedItem::Post { post, .. } => format!("post:{}", post.id),
        }
    }
}

/// What has to change in a list of `old` rows to make it `new`: the range to
/// replace and how many rows go in its place. `None` when nothing does.
///
/// The common prefix and the common suffix are left alone, which is the whole
/// point: an older page arriving is a splice at the top, a new message one at
/// the bottom, and in both cases every row the reader is looking at keeps its
/// identity and the list keeps its anchor.
fn splice_plan(old: &[String], new: &[String]) -> Option<(Range<usize>, usize)> {
    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let removed = prefix..old.len() - suffix;
    let added = new.len() - suffix - prefix;
    (!removed.is_empty() || added > 0).then_some((removed, added))
}

/// Whether a post belongs in the feed on screen at all — the question both
/// "should this row be there" and "should sending it move the reader" turn on.
fn feed_shows(showing: Option<&str>, post: &Post, crt: bool) -> bool {
    showing == Some(post.channel_id.as_str())
        && !post.is_deleted()
        && !post.is_system()
        && !(crt && post.is_reply())
}

/// Whether `post` groups with the post drawn immediately before it: same
/// author, same day, close enough in time. Author is compared by name rather
/// than user id because a webhook can post under a different name per message
/// with the same id.
#[cfg(test)]
fn groups_with(prev: Option<&Post>, post: &Post, st: &AppState) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    format_day(post.create_at) == format_day(prev.create_at)
        && st.author_name(post) == st.author_name(prev)
        && post.create_at.saturating_sub(prev.create_at) < message::GROUPING_WINDOW_MS
}

fn flush_system(run: &mut Vec<Post>, items: &mut Vec<FeedItem>, st: &AppState) {
    if run.is_empty() {
        return;
    }
    items.push(FeedItem::System {
        key: run[0].id.clone(),
        lines: Rc::new(
            message::system_lines(run, st)
                .into_iter()
                .map(SharedString::from)
                .collect(),
        ),
    });
    run.clear();
}

pub(super) fn build_feed_items(
    posts: &[Post],
    st: &AppState,
    crt: bool,
    unread_since: Option<Millis>,
    at_oldest_title: Option<&str>,
) -> Vec<FeedItem> {
    let mut items = Vec::with_capacity(posts.len() + 4);
    if let Some(title) = at_oldest_title.filter(|_| !posts.is_empty()) {
        items.push(FeedItem::Start(title.to_string()));
    }
    if posts.is_empty() {
        items.push(FeedItem::Empty);
        return items;
    }

    let mut unread_drawn = false;
    let mut last_author: Option<String> = None;
    let mut last_at: Millis = 0;
    let mut last_day: Option<String> = None;
    let mut system_run: Vec<Post> = Vec::new();

    for post in posts {
        if post.is_deleted() || (crt && post.is_reply()) {
            continue;
        }
        if unread_since.is_some_and(|at| post.create_at > at) && !unread_drawn {
            flush_system(&mut system_run, &mut items, st);
            items.push(FeedItem::Unread);
            unread_drawn = true;
        }

        let day = format_day(post.create_at);
        if last_day.as_deref() != Some(day.as_str()) {
            flush_system(&mut system_run, &mut items, st);
            items.push(FeedItem::Day(day.clone()));
            last_day = Some(day);
            last_author = None;
        }

        if post.is_system() {
            system_run.push(post.clone());
            last_author = None;
            continue;
        }

        flush_system(&mut system_run, &mut items, st);
        let author = st.author_name(post);
        let grouped = last_author.as_deref() == Some(author.as_str())
            && post.create_at.saturating_sub(last_at) < message::GROUPING_WINDOW_MS;
        items.push(FeedItem::Post {
            post: Rc::new(post.clone()),
            grouped,
            body: message::message_markdown(&post.message, st).into(),
        });
        last_author = Some(author);
        last_at = post.create_at;
    }
    flush_system(&mut system_run, &mut items, st);
    items
}

/// "Anna is typing…", for however many people are.
fn typing_text(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("{one} is typing…"),
        [one, two] => format!("{one} and {two} are typing…"),
        [one, two, ..] => format!("{one}, {two} and others are typing…"),
    }
}

/// The banner over a channel with a call in it.
fn call_banner_text(participants: usize) -> String {
    match participants {
        0 => "A call is starting".to_string(),
        1 => "1 person is in a call".to_string(),
        n => format!("{n} people are in a call"),
    }
}

/// The line under the channel name: its topic, and how many people are in it.
/// The count answers "who can see this" without opening the member list.
fn subtitle(header: &str, members: Option<i64>) -> String {
    let topic = header.lines().next().unwrap_or("").trim();
    match members {
        Some(n) if !topic.is_empty() => format!("{topic} · {n} members"),
        Some(n) => format!("{n} members"),
        None => topic.to_string(),
    }
}

/// The conversation pane's state.
pub struct ChatView {
    window: Rc<WindowSlot>,
    /// The scrolling list. It exists without a window — it is only bookkeeping
    /// about rows and where the reader is among them.
    list: ListState,
    items: RefCell<Vec<FeedItem>>,
    /// Which channel the feed currently holds, so a redraw can tell itself
    /// apart from a channel switch.
    showing: RefCell<Option<String>>,
    /// Shown instead of an empty feed while the first page is in flight, so a
    /// slow channel reads as loading rather than as empty.
    loading: Cell<bool>,
    /// Set while the page before this one is fetched.
    loading_older: Cell<bool>,
    /// Edge-trigger for history pagination. A physical approach to the top
    /// produces one request, not one request per scroll tick.
    pagination_armed: Rc<Cell<bool>>,
    typing: RefCell<Vec<String>>,
    /// `None` means connected. Anything else is shown until it is cleared.
    connection: RefCell<Option<String>>,
    member_count: Cell<Option<i64>>,
    /// The agent menu: (target, label) per bot.
    agents: RefCell<Vec<(String, String)>>,
    uploading: Cell<usize>,
    /// The post being edited, when the composer is in edit mode.
    editing: RefCell<Option<String>>,
    /// The priority chosen for the next message: "", "important" or "urgent".
    priority: RefCell<String>,
    completions: RefCell<Completions>,
    calls_available: Cell<bool>,
    calls_reason: RefCell<Option<String>>,
    in_call: Cell<bool>,
    call_ongoing: Cell<bool>,
    call_participants: Cell<Option<usize>>,
    /// The message something just navigated to, marked out for a moment.
    highlight: RefCell<Option<String>>,
    /// The composer, while there is a window to put it in.
    composer: RefCell<Option<Entity<TextareaState>>>,
    /// Set while a draft is being put back, so the change it causes is not
    /// mistaken for the user typing.
    restoring: Cell<bool>,
}

impl ChatView {
    pub fn new(window: Rc<WindowSlot>) -> Self {
        let list = ListState::new(0, ListAlignment::Bottom, OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        ChatView {
            window,
            list,
            items: RefCell::new(Vec::new()),
            showing: RefCell::new(None),
            loading: Cell::new(false),
            loading_older: Cell::new(false),
            pagination_armed: Rc::new(Cell::new(true)),
            typing: RefCell::new(Vec::new()),
            connection: RefCell::new(None),
            member_count: Cell::new(None),
            agents: RefCell::new(Vec::new()),
            uploading: Cell::new(0),
            editing: RefCell::new(None),
            priority: RefCell::new(String::new()),
            completions: RefCell::new(Completions::default()),
            calls_available: Cell::new(false),
            calls_reason: RefCell::new(None),
            in_call: Cell::new(false),
            call_ongoing: Cell::new(false),
            call_participants: Cell::new(None),
            highlight: RefCell::new(None),
            composer: RefCell::new(None),
            restoring: Cell::new(false),
        }
    }

    /// Wires the list's scrolling to the session. Done once, when the session
    /// is built: the handler outlives every window.
    pub(super) fn connect(&self, ui: &Rc<Ui>) {
        let weak = Rc::downgrade(ui);
        let armed = self.pagination_armed.clone();
        self.list.set_scroll_handler(move |event, _, cx| {
            if scroll_trace_enabled() {
                tracing::info!(
                    target: "matterfast::scroll",
                    event = "scrolled",
                    first = event.visible_range.start,
                    last = event.visible_range.end,
                    following = event.is_following_tail,
                    "scroll trace"
                );
            }
            // Away from the top again: the next approach is a new request.
            if event.is_following_tail || event.visible_range.start > PAGINATE_WITHIN * 4 {
                armed.set(true);
            }
            if !event.is_following_tail
                && event.visible_range.start <= PAGINATE_WITHIN
                && armed.replace(false)
            {
                let Some(ui) = weak.upgrade() else { return };
                ui.dispatch(Action::LoadOlder, cx);
            }
        });
    }

    /// Gives the pane its composer. Called when a window is built.
    pub(super) fn attach(&self, composer: Entity<TextareaState>) {
        *self.composer.borrow_mut() = Some(composer);
    }

    /// The window went away; the composer went with it.
    pub(super) fn detach(&self) {
        self.composer.borrow_mut().take();
    }

    // ------------------------------------------------------------ small state

    /// Marks the first page of a channel as in flight. Only matters while
    /// there is nothing to show: a reload over existing messages should leave
    /// them on screen rather than blank the pane.
    pub fn set_loading(&self, loading: bool, cx: &mut App) {
        self.loading.set(loading);
        cx.refresh_windows();
    }

    pub fn set_typing(&self, names: &[String], cx: &mut App) {
        if *self.typing.borrow() != names {
            *self.typing.borrow_mut() = names.to_vec();
            cx.refresh_windows();
        }
    }

    /// `None` means connected. Anything else is shown until it is cleared.
    pub fn set_connection_problem(&self, problem: Option<&str>, cx: &mut App) {
        *self.connection.borrow_mut() = problem.map(str::to_string);
        cx.refresh_windows();
    }

    /// How many people are in the channel, shown beside the topic.
    pub fn set_member_count(&self, count: Option<i64>, cx: &mut App) {
        self.member_count.set(count);
        cx.refresh_windows();
    }

    /// Rebuilds the agent menu: one entry to summarise this channel, and one
    /// per bot to go and talk to it. `bots` is (target, label).
    pub fn set_agents(&self, bots: &[(String, String)], cx: &mut App) {
        *self.agents.borrow_mut() = bots.to_vec();
        cx.refresh_windows();
    }

    /// A file being uploaded shows as a chip that is not yet removable.
    pub fn set_uploading(&self, count: usize, cx: &mut App) {
        self.uploading.set(self.uploading.get() + count);
        cx.refresh_windows();
    }

    /// One of the uploads in flight has landed, or failed.
    pub fn upload_finished(&self, cx: &mut App) {
        self.uploading.set(self.uploading.get().saturating_sub(1));
        cx.refresh_windows();
    }

    /// Puts the composer into edit mode for an existing post.
    ///
    /// Editing reuses the composer rather than opening a second one: there is
    /// only ever one message being written at a time, and a separate box would
    /// be a second place to lose text in.
    pub fn begin_edit(&self, post_id: &str, text: &str, cx: &mut App) {
        *self.editing.borrow_mut() = Some(post_id.to_string());
        self.set_composer_text(text, cx);
        self.focus_composer(cx);
    }

    /// Leaves edit mode, clearing the composer.
    pub fn end_edit(&self, cx: &mut App) {
        *self.editing.borrow_mut() = None;
        self.set_composer_text("", cx);
    }

    /// The post being edited, if any.
    pub fn editing(&self, _cx: &App) -> Option<String> {
        self.editing.borrow().clone()
    }

    /// Answers an outstanding completion query.
    pub fn set_completions(&self, items: Vec<Candidate>, cx: &mut App) {
        self.completions.borrow_mut().set(items);
        cx.refresh_windows();
    }

    /// The priority for the message being written: "", "important" or
    /// "urgent". Empty means standard, which is what the server expects.
    pub fn priority(&self, _cx: &App) -> String {
        self.priority.borrow().clone()
    }

    /// Back to standard once a message has gone out. Priority is per message,
    /// and a sticky "urgent" would quietly escalate everything after it.
    pub fn reset_priority(&self, cx: &mut App) {
        self.priority.borrow_mut().clear();
        cx.refresh_windows();
    }

    fn set_priority(&self, priority: &str, cx: &mut App) {
        *self.priority.borrow_mut() = match priority {
            "important" | "urgent" => priority.to_string(),
            _ => String::new(),
        };
        cx.refresh_windows();
    }

    pub fn set_calls_available(&self, available: bool, reason: Option<&str>, cx: &mut App) {
        self.calls_available.set(available);
        *self.calls_reason.borrow_mut() = reason.map(str::to_string);
        cx.refresh_windows();
    }

    /// Reflects our own membership. Everything you can *do* inside a call is
    /// in the dock; the header only starts, joins or ends one.
    pub fn set_in_call(&self, in_call: bool, ongoing: bool, cx: &mut App) {
        self.in_call.set(in_call);
        self.call_ongoing.set(ongoing);
        cx.refresh_windows();
    }

    pub fn set_call_in_progress(&self, participants: Option<usize>, cx: &mut App) {
        self.call_participants.set(participants);
        cx.refresh_windows();
    }

    /// Says, at the top of the feed, that the page before this one is on its
    /// way. Without it a scrollback that takes a moment looks like the start
    /// of the channel.
    pub fn set_loading_older(&self, loading: bool, cx: &mut App) {
        self.loading_older.set(loading);
        cx.refresh_windows();
    }

    /// A network error should be retryable on the next scroll even if the
    /// reader is still close to the history edge.
    pub fn retry_older_on_next_edge_change(&self, _cx: &App) {
        self.pagination_armed.set(true);
    }

    // --------------------------------------------------------------- composer

    /// What is in the composer right now.
    pub fn composer_text(&self, cx: &App) -> String {
        self.composer
            .borrow()
            .as_ref()
            .map(|composer| composer.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Puts a draft back. Setting the text fires a change event, which would
    /// otherwise be read as the user typing and save the draft straight back —
    /// hence the guard.
    pub fn set_composer_text(&self, text: &str, cx: &mut App) {
        if self.composer_text(cx) == text {
            return;
        }
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        self.restoring.set(true);
        let text = text.to_string();
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| {
                composer.set_value(text.clone(), window, cx);
                // Put the cursor where they left off, not at the front.
                composer.set_selected_range(text.len()..text.len(), cx);
            });
        });
        self.restoring.set(false);
        self.completions.borrow_mut().close();
    }

    pub fn focus_composer(&self, cx: &mut App) {
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| composer.focus(window, cx));
        });
    }

    /// The composer's text changed. Works out what, if anything, is being
    /// completed, and says so.
    pub(super) fn composer_changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        if self.restoring.get() {
            return;
        }
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        let (text, cursor) = {
            let composer = composer.read(cx);
            (composer.value().to_string(), composer.cursor())
        };
        let query = autocomplete::token_at(&text, cursor.min(text.len()));
        let changed = self.completions.borrow().query != query;
        if changed {
            {
                let mut completions = self.completions.borrow_mut();
                if query.is_none() {
                    completions.close();
                }
                completions.query = query.clone();
            }
            ui.dispatch(Action::Complete(query), cx);
        }
        // Restoring a draft is not typing, and was returned from above.
        ui.dispatch(Action::ComposerChanged(!text.is_empty()), cx);
    }

    /// Enter in the composer: picks the selected candidate while the list is
    /// up, and sends otherwise.
    pub(super) fn composer_submitted(&self, ui: &Rc<Ui>, cx: &mut App) {
        if self.accept_completion(ui, cx) {
            return;
        }
        self.submit(ui, cx);
    }

    fn submit(&self, ui: &Rc<Ui>, cx: &mut App) {
        let text = self.composer_text(cx).trim().to_string();
        // An empty message with files waiting is still a message; the session
        // decides, since it is what knows about the files.
        if text.is_empty() && !ui.has_pending_files() {
            return;
        }
        self.set_composer_text("", cx);
        ui.dispatch(Action::Send(text), cx);
    }

    /// Replaces the token under the cursor with the selected candidate.
    /// `false` when the list is not open, so the key means what it usually
    /// does.
    fn accept_completion(&self, ui: &Rc<Ui>, cx: &mut App) -> bool {
        let Some(insert) = self
            .completions
            .borrow()
            .is_open()
            .then(|| self.completions.borrow().chosen())
            .flatten()
        else {
            return false;
        };
        let Some(composer) = self.composer.borrow().clone() else {
            return false;
        };
        let (text, cursor) = {
            let composer = composer.read(cx);
            (composer.value().to_string(), composer.cursor())
        };
        let (text, caret) = autocomplete::accept(&text, cursor, &insert);
        self.completions.borrow_mut().close();
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| {
                composer.set_value(text.clone(), window, cx);
                composer.set_selected_range(caret..caret, cx);
            });
        });
        // The text did change, and the draft should follow it.
        ui.dispatch(Action::ComposerChanged(true), cx);
        cx.refresh_windows();
        true
    }

    /// Whether the completion list is up, and so has first refusal on keys.
    pub(super) fn completing(&self) -> bool {
        self.completions.borrow().is_open()
    }

    // ------------------------------------------------------------------- feed

    /// The row a post is drawn in, if it is in the feed at all.
    fn index_of(&self, post_id: &str) -> Option<usize> {
        self.items
            .borrow()
            .iter()
            .position(|item| item.post_id() == Some(post_id))
    }

    /// Scrolls the feed so a particular message is in view and briefly
    /// highlights it. Returns false when that message is not in the feed — the
    /// caller then knows it has to fetch further back first.
    pub fn scroll_to_post(&self, post_id: &str, cx: &mut App) -> bool {
        let Some(index) = self.index_of(post_id) else {
            return false;
        };
        if scroll_trace_enabled() {
            tracing::warn!(
                target: "matterfast::scroll",
                event = "programmatic-scroll-to",
                reason = "explicit-post-navigation",
                post_id,
                index,
                "scroll trace"
            );
        }
        self.scroll_near(index);
        *self.highlight.borrow_mut() = Some(post_id.to_string());
        cx.refresh_windows();

        let id = post_id.to_string();
        crate::runtime::after(std::time::Duration::from_secs(2), move |cx| {
            let Some(ui) = super::current(cx) else { return };
            let mut highlight = ui.chat.highlight.borrow_mut();
            if highlight.as_deref() == Some(id.as_str()) {
                *highlight = None;
                cx.refresh_windows();
            }
        });
        true
    }

    /// Puts a row a little way down from the top of the viewport, with what
    /// led up to it still visible above.
    fn scroll_near(&self, index: usize) {
        // Starting a row or two early is how "not jammed against the top
        // edge" is spelled in a list that scrolls by item.
        self.list.scroll_to(ListOffset {
            item_ix: index.saturating_sub(2),
            offset_in_item: px(0.),
        });
    }

    /// The message the reader is at, as (channel, post id). The post is `None`
    /// at the live bottom. A post id rather than pixels, so the position
    /// survives fonts, window width and late media layout.
    pub fn current_anchor(&self, _cx: &App) -> Option<(String, Option<String>)> {
        let channel_id = self.showing.borrow().clone()?;
        if self.list.is_following_tail() {
            return Some((channel_id, None));
        }
        let top = self.list.logical_scroll_top().item_ix;
        let items = self.items.borrow();
        // A couple of rows in from the top: the first row may be mostly
        // scrolled away, and a day separator is not a message.
        let post_id = items
            .iter()
            .skip(top + 2)
            .chain(items.iter().skip(top))
            .find_map(|item| item.post_id().map(str::to_string));
        Some((channel_id, post_id))
    }

    /// Goes back to a saved message. This is an explicit navigation, not
    /// pagination compensation.
    pub fn restore_anchor(&self, post_id: &str, cx: &mut App) -> bool {
        let Some(index) = self.index_of(post_id) else {
            return false;
        };
        if scroll_trace_enabled() {
            tracing::warn!(
                target: "matterfast::scroll",
                event = "programmatic-scroll-to",
                reason = "restore-saved-anchor",
                post_id,
                index,
                "scroll trace"
            );
        }
        self.scroll_near(index);
        cx.refresh_windows();
        true
    }

    /// Puts the feed on its newest row and keeps it there.
    fn scroll_to_newest(&self, reason: &str) {
        if scroll_trace_enabled() {
            tracing::warn!(
                target: "matterfast::scroll",
                event = "programmatic-scroll-to",
                reason,
                "scroll trace"
            );
        }
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// Puts the feed at the bottom because *you* posted. Following only when
    /// the reader was already at the live edge is the right rule for someone
    /// else's message and the wrong one for your own: every other client
    /// shows you what you just sent, wherever you had been reading.
    ///
    /// Does nothing when the post is not in this feed (a reply CRT keeps in
    /// its thread, or a thread whose root lives in another channel), so
    /// answering in the thread panel does not drag the channel behind it.
    pub fn follow_own_post(&self, post: &Post, state: &SharedState, cx: &mut App) {
        if !feed_shows(
            self.showing.borrow().as_deref(),
            post,
            state.borrow().crt_enabled,
        ) {
            return;
        }
        self.scroll_to_newest("own-message-sent");
        cx.refresh_windows();
    }

    /// Redraws the feed for the current channel.
    ///
    /// The rows are rebuilt from the state and compared with what the list
    /// held: whatever is common at either end keeps its place, so an older
    /// page arriving, a new message, an edit and a reaction are all the same
    /// operation, and none of them moves a reader who is not at the bottom.
    pub fn refresh(&self, state: &SharedState, cx: &mut App) {
        let st = state.borrow();
        let channel = st
            .current_channel
            .clone()
            .and_then(|id| st.channel(&id).cloned());
        let Some(channel) = channel else {
            drop(st);
            if self.showing.borrow_mut().take().is_some() {
                self.items.borrow_mut().clear();
                self.list.reset(0);
            }
            cx.refresh_windows();
            return;
        };
        let channel_id = channel.id.clone();

        // Only when something is actually unread: the line is a landmark, not
        // a permanent divider, and it must not sit under every channel you
        // have already read.
        let unread_since = st
            .memberships
            .get(&channel_id)
            .filter(|_| st.unread(&channel_id).is_unread())
            .and_then(|m| m.last_viewed_at)
            .filter(|at| *at > 0);

        let empty = crate::state::ChannelFeed::default();
        let feed = st.feeds.get(&channel_id).unwrap_or(&empty);
        let at_latest = feed.at_latest || feed.posts.is_empty();
        let title = st.channel_title(&channel);
        let items = build_feed_items(
            &feed.posts,
            &st,
            st.crt_enabled,
            unread_since,
            feed.at_oldest.then_some(title.as_str()),
        );
        drop(st);

        // Whether this is a redraw of what is already on screen, as opposed
        // to arriving in a different channel — the scroll position is only
        // worth keeping in the first case.
        let same_channel = self.showing.borrow().as_deref() == Some(channel_id.as_str());
        if scroll_trace_enabled() {
            tracing::info!(
                target: "matterfast::scroll",
                event = "feed-refresh",
                channel_id,
                same_channel,
                at_latest,
                following = self.list.is_following_tail(),
                rows = items.len(),
                "scroll trace"
            );
        }

        if same_channel {
            let old: Vec<String> = self.items.borrow().iter().map(FeedItem::key).collect();
            let new: Vec<String> = items.iter().map(FeedItem::key).collect();
            *self.items.borrow_mut() = items;
            if let Some((removed, added)) = splice_plan(&old, &new) {
                self.list.splice(removed, added);
            }
        } else {
            *self.showing.borrow_mut() = Some(channel_id);
            self.pagination_armed.set(true);
            self.member_count.set(None);
            self.highlight.borrow_mut().take();
            let unread_at = items
                .iter()
                .position(|item| matches!(item, FeedItem::Unread));
            self.list.reset(items.len());
            *self.items.borrow_mut() = items;
            match unread_at {
                // A block of history that does not reach the newest message
                // opens where reading stopped, which is what it was fetched
                // around; scrolling to its end would land in the past.
                Some(index) if !at_latest => self.scroll_near(index),
                _ => self.scroll_to_newest("channel-open"),
            }
        }
        cx.refresh_windows();
    }
}

// -------------------------------------------------------------------- drawing

fn render_item(ui: &Rc<Ui>, item: &FeedItem, highlight: Option<&str>, cx: &mut App) -> AnyElement {
    match item {
        FeedItem::Start(title) => div()
            .px_3()
            .pb_2()
            .text_color(cx.theme().muted_foreground)
            .child(format!("This is the beginning of {title}"))
            .into_any_element(),
        FeedItem::Empty => div()
            .h(px(320.))
            .child(kit::empty_state(
                Lucide::MessageSquarePlus,
                "No messages yet",
                "Say something to get started.",
                cx,
            ))
            .into_any_element(),
        FeedItem::Day(day) => div()
            .px_3()
            .pt_2()
            .child(kit::labelled_rule(day.clone(), cx.theme().border, cx))
            .into_any_element(),
        FeedItem::Unread => div()
            .px_3()
            .pt_2()
            .child(kit::labelled_rule("New messages", cx.theme().danger, cx))
            .into_any_element(),
        FeedItem::System { key, lines } => message::system_block(key, lines, cx),
        FeedItem::Post {
            post,
            grouped,
            body,
        } => message::row(
            ui,
            post,
            body,
            RowOptions {
                grouped: *grouped,
                show_thread_footer: true,
                highlight: highlight == Some(post.id.as_str()),
            },
            cx,
        ),
    }
}

/// The header: where you are, and what you can do to it.
fn header(ui: &Rc<Ui>, narrow: bool, cx: &mut App) -> AnyElement {
    let chat = &ui.chat;
    let st = ui.state.borrow();
    let channel = st
        .current_channel
        .as_ref()
        .and_then(|id| st.channel(id))
        .cloned();
    let title = channel
        .as_ref()
        .map(|c| st.channel_title(c))
        .unwrap_or_default();
    let subtitle = channel
        .as_ref()
        .map(|c| subtitle(&c.header, chat.member_count.get()))
        .unwrap_or_default();
    let inbox_count = (st
        .mentions
        .len()
        .min(99)
        .max(st.unread_threads().max(0) as usize) as i64)
        + st.reaction_unread;
    drop(st);

    let theme = cx.theme();
    let mut bar = h_flex()
        .flex_none()
        .w_full()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.border);

    if narrow {
        // The channel list is a page behind this one, and this is the way
        // back to it.
        bar = bar.child(
            kit::icon_button("back", Lucide::ChevronLeft, "Channels")
                .on_click(ui.click(|ui, cx| ui.split.set_show_content(false, cx))),
        );
    }

    bar = bar.child(
        v_flex()
            .flex_1()
            .min_w_0()
            .px_1()
            .child(
                div()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(subtitle),
                )
            }),
    );

    // The inbox button carries its own count, the way a mail client's does:
    // the number is the reason to click it.
    bar = bar.child(
        h_flex()
            .gap_0p5()
            .items_center()
            .child(
                kit::icon_button(
                    "inbox",
                    Lucide::Inbox,
                    if inbox_count > 0 {
                        format!("{inbox_count} unread — mentions and threads")
                    } else {
                        "Mentions and threads".to_string()
                    },
                )
                .on_click(ui.click(|ui, cx| ui.dispatch(Action::OpenInbox, cx))),
            )
            .when(inbox_count > 0, |row| {
                row.child(kit::mention_badge(inbox_count, false, cx))
            }),
    );

    // Only there when a server actually has an agent to ask. A menu rather
    // than a button because there are two different things to want from an
    // agent: a summary of here, or a conversation with it.
    let agents = chat.agents.borrow().clone();
    if !agents.is_empty() {
        let ui = ui.clone();
        bar = bar.child(
            kit::icon_button("agents", Lucide::Bot, "Agents").dropdown_menu(
                move |mut menu, _, _| {
                    menu = menu
                        .item(PopupMenuItem::new("Catch me up").on_click(
                            ui.click(|ui, cx| ui.dispatch(Action::SummariseUnreads, cx)),
                        ))
                        .separator();
                    for (target, label) in &agents {
                        let target = target.clone();
                        menu = menu.item(PopupMenuItem::new(label.clone()).on_click(ui.click(
                            move |ui, cx| match target.split_once(':') {
                                Some(("channel", id)) => {
                                    ui.dispatch(Action::SelectChannel(id.to_string()), cx)
                                }
                                Some(("user", id)) => {
                                    ui.dispatch(Action::OpenDirectMessage(id.to_string()), cx)
                                }
                                _ => {}
                            },
                        )));
                    }
                    menu
                },
            ),
        );
    }

    let in_call = chat.in_call.get();
    let ongoing = chat.call_ongoing.get();
    let call_tooltip = chat.calls_reason.borrow().clone().unwrap_or_else(|| {
        match (in_call, ongoing) {
            (true, _) => "Leave the call",
            // A call is running here and we are outside it: the button
            // joins, which is not something an icon alone ever says.
            (false, true) => "Join the call",
            (false, false) => "Start a call",
        }
        .to_string()
    });
    bar = bar.child(
        Button::new("call")
            .icon(if in_call { Lucide::PhoneOff } else { Lucide::Phone })
            .small()
            .tooltip(call_tooltip)
            // Red while it would hang up, accent while a call is waiting for
            // you, so the header agrees with the banner instead of looking
            // idle.
            .when(in_call, |button| button.danger())
            .when(!in_call && ongoing, |button| button.primary())
            .when(!in_call && !ongoing, |button| button.ghost())
            .disabled(!in_call && !chat.calls_available.get())
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleCall, cx))),
    );

    // What you do to *this* channel, as opposed to the message under the
    // pointer or the list in the sidebar.
    let menu_ui = ui.clone();
    bar = bar.child(
        kit::icon_button("channel-menu", Lucide::Ellipsis, "Channel menu").dropdown_menu(
            move |mut menu, _, _| {
                for (label, action) in [
                    ("Channel Details…", MenuAction::EditChannel),
                    ("Members…", MenuAction::ChannelMembers),
                    ("Bookmarks…", MenuAction::ChannelBookmarks),
                    ("Pinned Messages", MenuAction::PinnedPosts),
                    ("Notifications…", MenuAction::ChannelNotifications),
                    ("Leave Channel", MenuAction::LeaveChannel),
                    ("Archive Channel", MenuAction::ArchiveChannel),
                ] {
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .on_click(menu_ui.click(move |ui, cx| ui.menu_action(action, cx))),
                    );
                }
                menu
            },
        ),
    );

    bar.into_any_element()
}

/// A strip across the pane that says something is true until it stops being
/// true: the socket is down, a call is running, a message is being edited.
fn banner(text: impl Into<SharedString>, color: gpui_kit::Hsla, cx: &App) -> gpui_kit::Div {
    h_flex()
        .flex_none()
        .w_full()
        .px_3()
        .py_1p5()
        .gap_2()
        .items_center()
        .text_sm()
        .bg(color.opacity(0.14))
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().flex_1().min_w_0().child(text.into()))
}

/// The files waiting to go out with the next message.
fn attachments(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let files = ui.state.borrow().pending_files.clone();
    let uploading = ui.chat.uploading.get();
    if files.is_empty() && uploading == 0 {
        return None;
    }
    let chip = || {
        h_flex()
            .gap_1()
            .px_2()
            .h(px(28.))
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted.opacity(0.5))
            .text_sm()
    };
    let mut row = h_flex().flex_none().flex_wrap().gap_1p5().px_3p5().pb_1();
    for (index, (id, name)) in files.into_iter().enumerate() {
        row = row.child(
            chip()
                .child(Lucide::Paperclip)
                .child(div().max_w(px(220.)).truncate().child(name))
                .child(
                    kit::icon_button(("remove", index), Lucide::X, "Remove")
                        .xsmall()
                        .on_click(ui.click(move |ui, cx| {
                            ui.dispatch(Action::DropAttachment(id.clone()), cx)
                        })),
                ),
        );
    }
    if uploading > 0 {
        row = row.child(chip().child(Spinner::new().small()).child(match uploading {
            1 => "Uploading…".to_string(),
            n => format!("Uploading {n} files…"),
        }));
    }
    Some(row.into_any_element())
}

/// The completion list, floating over the bottom of the feed just above the
/// composer.
fn completion_list(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let completions = ui.chat.completions.borrow();
    if !completions.is_open() {
        return None;
    }
    let theme = cx.theme();
    let mut rows = v_flex()
        .id("completions")
        .absolute()
        .left_3()
        .bottom_1()
        .w(px(340.))
        .max_h(px(260.))
        .overflow_y_scroll()
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .shadow_md();
    for (index, item) in completions.items.iter().enumerate() {
        let selected = index == completions.selected;
        let mut row = h_flex()
            .id(("candidate", index))
            .gap_2()
            .px_2()
            .h(px(32.))
            .items_center()
            .rounded_sm()
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.accent))
            .hover(|style| style.bg(theme.list_hover));

        // A mention is a person and gets a face — initials until the picture
        // lands, as everywhere else, so the rows stay aligned. An emoji is
        // its own picture and gets none.
        if item.insert.starts_with('@') {
            let avatar = gpui_kit::component::avatar::Avatar::new()
                .name(SharedString::from(item.primary.clone()))
                .with_size(gpui_kit::component::Size::Size(px(24.)));
            row = row.child(match &item.image {
                Some(picture) => avatar.src(picture.clone()),
                None => avatar,
            });
        }
        if let Some(name) = &item.emoji {
            row = row.child(super::message::emoji_element(ui, name, 20.));
        }
        row = row.child(div().flex_none().truncate().child(item.primary.clone()));
        if !item.secondary.is_empty() {
            row = row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_right()
                    .text_color(theme.muted_foreground)
                    .child(item.secondary.clone()),
            );
        }
        rows = rows.child(row.on_click(ui.click(move |ui, cx| {
            ui.chat.completions.borrow_mut().selected = index;
            ui.chat.accept_completion(ui, cx);
        })));
    }
    Some(rows.into_any_element())
}

/// The composer row.
fn composer(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let chat = &ui.chat;
    let Some(state) = chat.composer.borrow().clone() else {
        return div().into_any_element();
    };
    let priority = chat.priority(cx);
    let priority_ui = ui.clone();

    h_flex()
        .flex_none()
        .w_full()
        .gap_2()
        .px_3()
        .pt_1()
        .pb_3()
        .items_center()
        .child(
            kit::icon_button("attach", Lucide::Paperclip, "Attach a file")
                .with_size(Size::Medium)
                .on_click(ui.click(|ui, cx| ui.dispatch(Action::PickAttachment, cx))),
        )
        .child(
            // The button carries the current choice: a priority you set and
            // cannot see is one you will send by accident.
            Button::new("priority")
                .icon(Lucide::CircleAlert)
                .tooltip("Message priority")
                .when(priority == "urgent", |button| button.danger())
                .when(priority == "important", |button| button.primary())
                .when(priority.is_empty(), |button| button.ghost())
                .dropdown_menu(move |mut menu, _, _| {
                    for (label, value) in [
                        ("Standard", ""),
                        ("Important", "important"),
                        ("Urgent", "urgent"),
                    ] {
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(*priority_ui.chat.priority.borrow() == value)
                                .on_click(priority_ui.click(move |ui, cx| {
                                    ui.chat.set_priority(value, cx)
                                })),
                        );
                    }
                    menu
                }),
        )
        .child(
            kit::icon_button("schedule", Lucide::AlarmClock, "Send later")
                .with_size(Size::Medium)
                .on_click(ui.click(|ui, cx| ui.dispatch(Action::ScheduleMessage, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                // The completion list has first refusal on these keys: they
                // move through it while it is open, and only reach the text
                // when it is not.
                .capture_action({
                    let ui = ui.clone();
                    move |_: &MoveUp, _, cx| {
                        if ui.chat.completing() {
                            ui.chat.completions.borrow_mut().step(-1);
                            cx.stop_propagation();
                            cx.refresh_windows();
                        }
                    }
                })
                .capture_action({
                    let ui = ui.clone();
                    move |_: &MoveDown, _, cx| {
                        if ui.chat.completing() {
                            ui.chat.completions.borrow_mut().step(1);
                            cx.stop_propagation();
                            cx.refresh_windows();
                        }
                    }
                })
                // Enter picks from the list while it is up. Taken here, before
                // the text box sees the key: left to arrive as the box's own
                // "submitted", it had already been treated as typing.
                .capture_action({
                    let ui = ui.clone();
                    move |enter: &Enter, _, cx| {
                        if ui.chat.completing() && !enter.shift {
                            cx.stop_propagation();
                            ui.later(cx, |ui, cx| {
                                ui.chat.accept_completion(ui, cx);
                            });
                        }
                    }
                })
                .capture_action({
                    let ui = ui.clone();
                    move |_: &IndentInline, _, cx| {
                        if ui.chat.completing() {
                            cx.stop_propagation();
                            ui.later(cx, |ui, cx| {
                                ui.chat.accept_completion(ui, cx);
                            });
                        }
                    }
                })
                .capture_action({
                    let ui = ui.clone();
                    move |_: &Escape, _, cx| {
                        if ui.chat.completing() {
                            ui.chat.completions.borrow_mut().close();
                            cx.stop_propagation();
                            cx.refresh_windows();
                            // Whatever is in flight for the closed query must
                            // not reopen the list.
                            ui.dispatch(Action::Complete(None), cx);
                        }
                    }
                })
                .child(Textarea::new(&state).on_paste({
                    let ui = ui.clone();
                    move |item, _, cx| paste_image(&ui, item, cx)
                })),
        )
        .child(
            Button::new("send")
                .icon(Lucide::SendHorizontal)
                .primary()
                .tooltip("Send  (Enter)")
                .on_click(ui.click(|ui, cx| ui.chat.submit(ui, cx))),
        )
        .into_any_element()
}

/// Ctrl+V with an image on the clipboard attaches it. Pasting a screenshot is
/// how most images get into a chat, and the alternative is saving it to disk
/// first for no reason. Answers whether the paste was taken: when there is no
/// image, the text paste has to go through untouched.
fn paste_image(ui: &Rc<Ui>, item: &gpui_kit::ClipboardItem, cx: &mut App) -> bool {
    let Some(image) = item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image.clone()),
        _ => None,
    }) else {
        return false;
    };
    let extension = match image.format {
        gpui_kit::ImageFormat::Png => "png",
        gpui_kit::ImageFormat::Jpeg => "jpg",
        gpui_kit::ImageFormat::Webp => "webp",
        gpui_kit::ImageFormat::Gif => "gif",
        gpui_kit::ImageFormat::Bmp => "bmp",
        _ => return false,
    };
    // The upload path takes paths, so the pasted image lands in a temp file
    // that the OS cleans up.
    let path = std::env::temp_dir().join(format!(
        "matterfast-paste-{}.{extension}",
        crate::timefmt::unique()
    ));
    if let Err(error) = std::fs::write(&path, &image.bytes) {
        tracing::warn!(%error, "could not save the pasted image");
        return false;
    }
    ui.dispatch(Action::AttachFiles(vec![path]), cx);
    true
}

/// The feed itself: the list, its scrollbar, and what floats over it.
fn feed(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let chat = &ui.chat;
    let list_state = chat.list.clone();
    let row_ui = ui.clone();
    let rows = list(list_state.clone(), move |index, _, cx| {
        let chat = &row_ui.chat;
        let items = chat.items.borrow();
        let highlight = chat.highlight.borrow();
        match items.get(index) {
            Some(item) => render_item(&row_ui, item, highlight.as_deref(), cx),
            // The list was told about a row the feed no longer has; one frame
            // later it will not be asked for.
            None => div().into_any_element(),
        }
    })
    .size_full()
    .py_3();

    let scrolled_up = !list_state.is_following_tail()
        && list_state.max_offset_for_scrollbar().y > px(0.)
        && !list_state.is_scrolled_to_end().unwrap_or(false);

    div()
        .id("feed")
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .child(
            div()
                .size_full()
                .child(rows)
                .vertical_scrollbar(&list_state),
        )
        .when(chat.loading_older.get(), |feed| {
            feed.child(
                div()
                    .absolute()
                    .top_2()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(Spinner::new()),
            )
        })
        .when(scrolled_up, |feed| {
            feed.child(
                div()
                    .absolute()
                    .right_4()
                    .bottom_3()
                    .child(
                        Button::new("jump-to-latest")
                            .icon(Lucide::ArrowDown)
                            .tooltip("Jump to latest")
                            .on_click(ui.click(|ui, cx| {
                                ui.chat.scroll_to_newest("jump-button");
                                cx.refresh_windows();
                            })),
                    ),
            )
        })
        .when_some(completion_list(ui, cx), |feed, list| feed.child(list))
        .into_any_element()
}

/// Draws the conversation pane. `narrow` is whether the channel list is a
/// page behind this one rather than a column beside it; `dock` is the call
/// dock, when it belongs under the conversation instead of the sidebar.
pub fn render(ui: &Rc<Ui>, narrow: bool, dock: Option<AnyElement>, cx: &mut App) -> AnyElement {
    let chat = &ui.chat;
    let (has_channel, waiting) = {
        let st = ui.state.borrow();
        let channel = st
            .current_channel
            .as_ref()
            .filter(|id| st.channel(id).is_some());
        // A channel with no posts *yet* and a fetch in flight is loading; one
        // with no posts and nothing in flight is genuinely empty.
        let waiting = channel.is_some_and(|id| {
            chat.loading.get() && st.feeds.get(id).is_none_or(|feed| feed.posts.is_empty())
        });
        (channel.is_some(), waiting)
    };

    let mut pane = v_flex()
        .id("conversation")
        .size_full()
        .min_w_0()
        .bg(cx.theme().background)
        .child(header(ui, narrow, cx));

    if !has_channel {
        return pane
            .child(kit::empty_state(
                Lucide::MessageSquarePlus,
                "No channel selected",
                "Pick a channel from the sidebar to start reading.",
                cx,
            ))
            .when_some(dock, |pane, dock| pane.child(dock))
            .into_any_element();
    }

    // Losing the socket is a state of the whole window, not an event, so it
    // gets a banner that stays up rather than a toast that scrolls by.
    if let Some(problem) = chat.connection.borrow().clone() {
        pane = pane.child(banner(problem, cx.theme().warning, cx));
    }

    if let Some(participants) = chat.call_participants.get() {
        let mut call = banner(call_banner_text(participants), cx.theme().success, cx);
        // The banner is where you notice a call, so it is where joining it
        // belongs — the header button is for starting one.
        if !chat.in_call.get() {
            call = call.child(
                Button::new("join-call")
                    .label("Join")
                    .small()
                    .primary()
                    .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleCall, cx))),
            );
        }
        pane = pane.child(call);
    }

    if waiting {
        pane = pane.child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large()),
        );
    } else {
        pane = pane.child(feed(ui, cx));
    }

    // Editing is a mode, and a mode you cannot see is a trap: the banner says
    // so and offers the way out.
    if chat.editing.borrow().is_some() {
        pane = pane.child(
            banner("Editing a message", cx.theme().primary, cx).child(
                Button::new("cancel-edit")
                    .label("Cancel")
                    .small()
                    .ghost()
                    .on_click(ui.click(|ui, cx| ui.chat.end_edit(cx))),
            ),
        );
    }

    // Sits between the feed and the composer, reserving no space when empty.
    let typing = typing_text(&chat.typing.borrow());
    if !typing.is_empty() {
        pane = pane.child(
            div()
                .flex_none()
                .px_3p5()
                .pb_0p5()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(typing),
        );
    }

    if let Some(row) = attachments(ui, cx) {
        pane = pane.child(row);
    }

    pane.child(composer(ui, cx))
        .when_some(dock, |pane, dock| pane.child(dock))
        // Dropping files anywhere over the conversation attaches them. The
        // target is the whole pane rather than the composer: aiming at a
        // one-line text box is a needlessly precise thing to ask of a drag.
        .on_drop({
            let ui = ui.clone();
            move |paths: &ExternalPaths, _, cx| {
                let paths = paths.paths().to_vec();
                if !paths.is_empty() {
                    ui.dispatch(Action::AttachFiles(paths), cx);
                }
            }
        })
        .into_any_element()
}

/// Builds the composer and wires what it reports to the session.
pub(super) fn build_composer(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<TextareaState>, gpui_kit::Subscription) {
    let composer = cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(1, 8)
            // Enter sends; Shift+Enter inserts a newline.
            .submit_on_enter(true)
            .placeholder("Write a message…")
    });
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&composer, move |_, event: &InputEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            InputEvent::Change => ui.later(cx, |ui, cx| ui.chat.composer_changed(ui, cx)),
            InputEvent::PressEnter { shift: false, .. } => {
                ui.later(cx, |ui, cx| ui.chat.composer_submitted(ui, cx))
            }
            _ => {}
        }
    });
    (composer, subscription)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::{ClientConfig, User};
    use mattermost_api::Client;

    fn state_with(users: &[User]) -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut app = AppState::new(client, User::default(), ClientConfig::default(), false);
        for user in users {
            app.users.insert(user.id.clone(), user.clone());
        }
        app
    }

    fn user(id: &str, username: &str) -> User {
        User {
            id: id.into(),
            username: username.into(),
            ..Default::default()
        }
    }

    fn post(user_id: &str, at: Millis) -> Post {
        Post {
            id: format!("p{at}"),
            user_id: user_id.into(),
            create_at: at,
            ..Default::default()
        }
    }

    #[test]
    fn same_author_within_the_window_groups() {
        let state = state_with(&[user("u1", "anna")]);
        let prev = post("u1", 1_000);
        let next = post("u1", 1_000 + message::GROUPING_WINDOW_MS - 1);
        assert!(groups_with(Some(&prev), &next, &state));
    }

    #[test]
    fn a_gap_past_the_window_does_not_group() {
        let state = state_with(&[user("u1", "anna")]);
        let prev = post("u1", 1_000);
        let next = post("u1", 1_000 + message::GROUPING_WINDOW_MS);
        assert!(!groups_with(Some(&prev), &next, &state));
    }

    #[test]
    fn a_different_author_never_groups_even_seconds_apart() {
        let state = state_with(&[user("u1", "anna"), user("u2", "bob")]);
        let prev = post("u1", 1_000);
        let next = post("u2", 1_001);
        assert!(!groups_with(Some(&prev), &next, &state));
    }

    #[test]
    fn nothing_before_it_never_groups() {
        let state = state_with(&[]);
        let next = post("u1", 1_000);
        assert!(!groups_with(None, &next, &state));
    }

    #[test]
    fn the_feed_only_shows_posts_that_belong_in_it() {
        let mine = Post {
            channel_id: "c1".into(),
            ..post("u1", 1_000)
        };
        assert!(feed_shows(Some("c1"), &mine, true));
        assert!(!feed_shows(Some("c2"), &mine, true));
        assert!(!feed_shows(None, &mine, true));

        // A reply is a thread's business while CRT is on, and the channel's
        // once it is off — sending one must move the reader in exactly the
        // second case.
        let reply = Post {
            root_id: "root".into(),
            ..mine.clone()
        };
        assert!(!feed_shows(Some("c1"), &reply, true));
        assert!(feed_shows(Some("c1"), &reply, false));
    }

    fn keys(feed: &[FeedItem]) -> Vec<String> {
        feed.iter().map(FeedItem::key).collect()
    }

    /// Posts a few seconds apart on one day, so the only rows are the day,
    /// the posts, and whatever the test adds.
    fn posts(ids: std::ops::Range<i64>) -> Vec<Post> {
        ids.map(|n| Post {
            id: format!("p{n}"),
            user_id: "u1".into(),
            create_at: 1_700_000_000_000 + n * 1_000,
            ..Default::default()
        })
        .collect()
    }

    #[test]
    fn the_feed_groups_and_separates_the_way_it_reads() {
        let state = state_with(&[user("u1", "anna")]);
        let feed = build_feed_items(&posts(0..3), &state, false, None, Some("Town Square"));
        assert!(matches!(feed[0], FeedItem::Start(_)));
        assert!(matches!(feed[1], FeedItem::Day(_)));
        // The first of a run names its author; the rest follow it.
        let grouped: Vec<bool> = feed
            .iter()
            .filter_map(|item| match item {
                FeedItem::Post { grouped, .. } => Some(*grouped),
                _ => None,
            })
            .collect();
        assert_eq!(grouped, [false, true, true]);

        // Nothing at all is its own page, not a start label over a void.
        let empty = build_feed_items(&[], &state, false, None, Some("Town Square"));
        assert!(matches!(empty.as_slice(), [FeedItem::Empty]));
    }

    #[test]
    fn the_unread_line_goes_where_reading_stopped() {
        let state = state_with(&[user("u1", "anna")]);
        let posts = posts(0..4);
        let feed = build_feed_items(&posts, &state, false, Some(posts[1].create_at), None);
        let at = feed
            .iter()
            .position(|item| matches!(item, FeedItem::Unread))
            .expect("an unread line");
        assert_eq!(feed[at + 1].post_id(), Some("p2"));
        // After the line the group starts over only if the author changed —
        // the line itself is not an author.
        assert_eq!(feed[at - 1].post_id(), Some("p1"));
    }

    #[test]
    fn replies_stay_out_of_the_feed_under_collapsed_threads() {
        let state = state_with(&[user("u1", "anna")]);
        let mut posts = posts(0..2);
        posts[1].root_id = "p0".into();
        assert_eq!(
            build_feed_items(&posts, &state, true, None, None)
                .iter()
                .filter(|item| item.post_id().is_some())
                .count(),
            1
        );
        assert_eq!(
            build_feed_items(&posts, &state, false, None, None)
                .iter()
                .filter(|item| item.post_id().is_some())
                .count(),
            2
        );
    }

    #[test]
    fn an_older_page_is_a_splice_at_the_top() {
        let state = state_with(&[user("u1", "anna")]);
        let before = keys(&build_feed_items(&posts(5..10), &state, false, None, None));
        let after = keys(&build_feed_items(&posts(0..10), &state, false, None, None));
        // Everything from the first old post down is untouched; only the day
        // row and the new posts above it are replaced.
        let (removed, added) = splice_plan(&before, &after).unwrap();
        assert_eq!(removed, 1..1, "nothing already on screen is rebuilt");
        assert_eq!(added, 5);
    }

    #[test]
    fn a_new_message_is_a_splice_at_the_bottom() {
        let state = state_with(&[user("u1", "anna")]);
        let before = keys(&build_feed_items(&posts(0..5), &state, false, None, None));
        let after = keys(&build_feed_items(&posts(0..6), &state, false, None, None));
        assert_eq!(splice_plan(&before, &after), Some((6..6, 1)));
    }

    #[test]
    fn an_edit_or_a_reaction_moves_nothing() {
        // The row is the same row; what it says is the drawing's business.
        let state = state_with(&[user("u1", "anna")]);
        let mut edited = posts(0..5);
        let before = keys(&build_feed_items(&edited, &state, false, None, None));
        edited[2].message = "changed".into();
        edited[2].edit_at = 5;
        let after = keys(&build_feed_items(&edited, &state, false, None, None));
        assert_eq!(splice_plan(&before, &after), None);
    }

    #[test]
    fn the_echo_of_a_sent_message_replaces_only_its_own_row() {
        let before = vec!["day:Today".to_string(), "post:a".into(), "post:pending1".into()];
        let after = vec!["day:Today".to_string(), "post:a".into(), "post:real".into()];
        assert_eq!(splice_plan(&before, &after), Some((2..3, 1)));
        // And a delete takes one row out.
        assert_eq!(splice_plan(&after, &after[..2].to_vec()), Some((2..3, 0)));
    }

    #[test]
    fn the_lines_around_the_feed_read_naturally() {
        assert_eq!(typing_text(&[]), "");
        assert_eq!(typing_text(&["Anna".into()]), "Anna is typing…");
        assert_eq!(
            typing_text(&["Anna".into(), "Bob".into(), "Carol".into()]),
            "Anna, Bob and others are typing…"
        );
        assert_eq!(call_banner_text(0), "A call is starting");
        assert_eq!(call_banner_text(3), "3 people are in a call");
        assert_eq!(subtitle("Release talk\nsecond line", Some(4)), "Release talk · 4 members");
        assert_eq!(subtitle("", Some(4)), "4 members");
        assert_eq!(subtitle("Release talk", None), "Release talk");
    }
}
