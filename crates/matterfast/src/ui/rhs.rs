//! The right-hand panel: a thread, the notification inbox, or search results.
//!
//! Mattermost puts all of these in the same place and swaps between them,
//! which is worth copying — they are alternatives, never side by side, and
//! sharing one surface keeps the window from growing a fifth column.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, list, point, px, AnyElement, App, Entity, FollowMode, FontWeight, ListAlignment,
    ListState, ScrollHandle, SharedString, Window,
};
use mattermost_api::models::{Millis, Post};

use super::kit::{self, Lucide};
use super::message::{self, RowOptions};
use super::{Action, Ui, WindowSlot};
use crate::state::{AppState, SharedState};
use crate::timefmt::format_relative;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelMode {
    Hidden,
    /// Viewing the thread rooted at this post id.
    Thread(String),
    /// Mentions and unread threads.
    Inbox,
    /// Results for a search, held so a redraw does not lose them.
    Search(String),
}

/// Which list of the inbox is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum InboxTab {
    #[default]
    Mentions,
    Threads,
    Saved,
}

/// One row of an open thread.
#[derive(Clone)]
enum ThreadRow {
    /// "3 replies", under the root. It is where Mattermost puts the reply
    /// count too, and it makes the thread readable at a glance.
    Divider(String),
    Post {
        post: Rc<Post>,
        grouped: bool,
        body: SharedString,
    },
}

impl ThreadRow {
    fn key(&self) -> String {
        match self {
            ThreadRow::Divider(_) => "divider".to_string(),
            ThreadRow::Post { post, .. } => format!("post:{}", post.id),
        }
    }
}

/// Where an inbox row goes when pressed.
#[derive(Clone)]
enum Target {
    /// A reply: its channel, and the thread it is in.
    Thread { channel_id: String, root_id: String },
    /// Not a reply, so there is no thread to open — the useful thing is the
    /// message itself.
    Message { channel_id: String, post_id: String },
    /// A followed thread, by its root.
    Followed(String),
}

/// One entry in the inbox, with everything it draws worked out already.
#[derive(Clone)]
struct InboxRow {
    user_id: String,
    author: String,
    channel: String,
    preview: SharedString,
    at: Millis,
    /// (replies, unread replies, unread mentions), for a followed thread.
    counts: Option<(i64, i64, i64)>,
    target: Target,
}

/// A search hit: the message, and which channel it came from.
#[derive(Clone)]
struct SearchRow {
    channel: String,
    post: Rc<Post>,
    body: SharedString,
}

#[derive(Default)]
struct Inbox {
    mentions: Vec<InboxRow>,
    threads: Vec<InboxRow>,
    saved: Vec<InboxRow>,
}

pub struct RightPanel {
    window: Rc<WindowSlot>,
    mode: RefCell<PanelMode>,
    /// The open thread's rows, and the channel it lives in.
    thread: RefCell<Vec<ThreadRow>>,
    thread_channel: RefCell<String>,
    /// False until the thread, or at least its root, is known.
    thread_loaded: Cell<bool>,
    list: ListState,
    following: Cell<bool>,
    tab: Cell<InboxTab>,
    inbox: RefCell<Inbox>,
    search: RefCell<Vec<SearchRow>>,
    searching: Cell<bool>,
    /// Whether the server may have hits past the ones listed.
    search_more: Cell<bool>,
    /// Where the list of hits is scrolled to: reaching its end is what asks
    /// for the next page.
    search_scroll: ScrollHandle,
    /// How many hits the last frame drew, which is what the scroll position
    /// was measured against.
    search_drawn: Cell<usize>,
    /// The reply box, while there is a window to put it in.
    composer: RefCell<Option<Entity<TextareaState>>>,
    /// Set while a draft is being restored, so it is not mistaken for typing.
    restoring: Cell<bool>,
}

impl RightPanel {
    pub fn new(window: Rc<WindowSlot>) -> Self {
        let list = ListState::new(0, ListAlignment::Bottom, px(400.));
        list.set_follow_mode(FollowMode::Tail);
        RightPanel {
            window,
            mode: RefCell::new(PanelMode::Hidden),
            thread: RefCell::new(Vec::new()),
            thread_channel: RefCell::new(String::new()),
            thread_loaded: Cell::new(false),
            list,
            following: Cell::new(false),
            tab: Cell::new(InboxTab::default()),
            inbox: RefCell::new(Inbox::default()),
            search: RefCell::new(Vec::new()),
            searching: Cell::new(false),
            search_more: Cell::new(false),
            search_scroll: ScrollHandle::new(),
            search_drawn: Cell::new(0),
            composer: RefCell::new(None),
            restoring: Cell::new(false),
        }
    }

    pub(super) fn attach(&self, composer: Entity<TextareaState>) {
        *self.composer.borrow_mut() = Some(composer);
    }

    pub(super) fn detach(&self) {
        self.composer.borrow_mut().take();
    }

    pub fn mode(&self, _cx: &App) -> PanelMode {
        self.mode.borrow().clone()
    }

    pub fn set_mode(&self, mode: PanelMode, cx: &mut App) {
        if *self.mode.borrow() != mode {
            // A different thread starts at its newest reply, not wherever the
            // last one was left.
            self.thread.borrow_mut().clear();
            self.thread_loaded.set(false);
            self.list.reset(0);
            self.list.set_follow_mode(FollowMode::Tail);
        }
        *self.mode.borrow_mut() = mode;
        cx.refresh_windows();
    }

    pub fn focus_composer(&self, cx: &mut App) {
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| composer.focus(window, cx));
        });
    }

    /// Reflects whether the open thread is followed.
    pub fn set_following(&self, following: bool, cx: &mut App) {
        if self.following.replace(following) != following {
            cx.refresh_windows();
        }
    }

    /// What is in the thread's reply box.
    pub fn composer_text(&self, cx: &App) -> String {
        self.composer
            .borrow()
            .as_ref()
            .map(|composer| composer.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Puts a thread draft back. Guarded like the channel composer's: setting
    /// the text fires a change, which must not be read as typing.
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
                composer.set_selected_range(text.len()..text.len(), cx);
            });
        });
        self.restoring.set(false);
    }

    /// Switches the inbox to its threads tab — used when it opens with unread
    /// threads and no mentions, which would otherwise land on an empty list.
    pub fn show_threads_tab(&self, cx: &mut App) {
        self.tab.set(InboxTab::Threads);
        cx.refresh_windows();
    }

    /// A new search starts at its newest hit, wherever the last one was left.
    pub(super) fn search_from_the_top(&self) {
        self.search_scroll.set_offset(point(px(0.), px(0.)));
    }

    /// Rebuilds whatever the panel is currently showing from the state.
    pub fn refresh(&self, state: &SharedState, cx: &mut App) {
        let st = state.borrow();
        match self.mode(cx) {
            PanelMode::Hidden => {}
            PanelMode::Thread(root_id) => self.refresh_thread(&root_id, &st),
            PanelMode::Inbox => *self.inbox.borrow_mut() = build_inbox(&st),
            PanelMode::Search(_) => {
                self.searching.set(st.searching);
                self.search_more.set(st.search_more);
                *self.search.borrow_mut() = st
                    .search_results
                    .iter()
                    .map(|post| SearchRow {
                        // Which channel a hit came from is most of what makes
                        // it useful.
                        channel: st
                            .channel(&post.channel_id)
                            .map(|c| st.channel_title(c))
                            .unwrap_or_default(),
                        post: Rc::new(post.clone()),
                        body: message::message_markdown(&post.message, &st).into(),
                    })
                    .collect();
            }
        }
        drop(st);
        cx.refresh_windows();
    }

    fn refresh_thread(&self, root_id: &str, st: &AppState) {
        // While the replies are in flight, show the root on its own if we
        // already hold it — the reader clicked a message they were looking at,
        // and an empty panel makes the click feel lost.
        let posts: Option<Vec<Post>> = match st.threads.get(root_id) {
            Some(feed) => Some(feed.posts.clone()),
            None => st.find_post(root_id).map(|root| vec![root.clone()]),
        };
        let Some(posts) = posts else {
            self.thread_loaded.set(false);
            return;
        };
        self.thread_loaded.set(true);
        *self.thread_channel.borrow_mut() = posts
            .first()
            .and_then(|p| st.channel(&p.channel_id))
            .map(|c| st.channel_title(c))
            .unwrap_or_default();

        let rows = build_thread_rows(posts, root_id, st);
        let old: Vec<String> = self.thread.borrow().iter().map(ThreadRow::key).collect();
        let new: Vec<String> = rows.iter().map(ThreadRow::key).collect();
        *self.thread.borrow_mut() = rows;
        if old.is_empty() {
            self.list.reset(new.len());
            self.list.set_follow_mode(FollowMode::Tail);
        } else if old != new {
            // A thread is a screenful, not a channel's history: the common
            // case is one reply added at the end.
            let common = old
                .iter()
                .zip(new.iter())
                .take_while(|(a, b)| a == b)
                .count();
            self.list.splice(common..old.len(), new.len() - common);
        }
    }

    /// The reply box changed; says so unless it was us putting a draft back.
    fn composer_changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        if !self.restoring.get() {
            ui.dispatch(Action::ThreadDraftChanged, cx);
        }
    }

    fn submit(&self, ui: &Rc<Ui>, cx: &mut App) {
        let text = self.composer_text(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        self.set_composer_text("", cx);
        ui.dispatch(Action::ReplyInThread(text), cx);
    }
}

/// A thread's rows: the root, the reply count, then the replies.
fn build_thread_rows(mut posts: Vec<Post>, root_id: &str, st: &AppState) -> Vec<ThreadRow> {
    // The feed arrives sorted by `create_at` (see `ChannelFeed::from_list`,
    // which has to sort because the thread endpoint does not). That is still
    // not enough: two posts can share a millisecond on a busy server, and then
    // the conversation starts with a reply. So the root goes first, always.
    if let Some(index) = posts.iter().position(|p| p.id == root_id) {
        let root = posts.remove(index);
        posts.insert(0, root);
    }
    posts.retain(|post| !post.is_deleted());
    let reply_count = posts.len().saturating_sub(1) as i64;

    let mut rows = Vec::with_capacity(posts.len() + 1);
    let mut last_author: Option<String> = None;
    let mut last_at: Millis = 0;
    for (index, post) in posts.into_iter().enumerate() {
        if index == 1 && reply_count > 0 {
            rows.push(ThreadRow::Divider(format!(
                "{reply_count} {}",
                message::plural(reply_count, "reply", "replies")
            )));
            last_author = None;
        }
        let author = st.author_name(&post);
        let grouped = index != 0
            && last_author.as_deref() == Some(author.as_str())
            && post.create_at - last_at < message::GROUPING_WINDOW_MS;
        last_author = Some(author);
        last_at = post.create_at;
        rows.push(ThreadRow::Post {
            body: message::message_markdown(&post.message, st).into(),
            post: Rc::new(post),
            grouped,
        });
    }
    rows
}

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

fn build_inbox(st: &AppState) -> Inbox {
    let mentions = st
        .mentions
        .iter()
        .map(|post| inbox_row_for(post, st))
        .collect();

    // Saved posts: the ones you flagged, newest first. They are held as a set
    // of ids, so this walks what is loaded rather than fetching — anything not
    // in memory shows up as soon as its channel is opened.
    let mut saved: Vec<&Post> = st
        .feeds
        .values()
        .chain(st.threads.values())
        .flat_map(|feed| feed.posts.iter())
        .filter(|p| st.saved_posts.contains(&p.id))
        .collect();
    saved.sort_by(|a, b| b.create_at.cmp(&a.create_at).then(a.id.cmp(&b.id)));
    // The same post can be held by a channel feed and by its thread.
    saved.dedup_by(|a, b| a.id == b.id);
    let saved = saved
        .into_iter()
        .map(|post| inbox_row_for(post, st))
        .collect();

    let threads = st
        .thread_inbox
        .iter()
        .map(|thread| InboxRow {
            user_id: thread.post.user_id.clone(),
            author: st.author_name(&thread.post),
            channel: st
                .channel(&thread.post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default(),
            preview: crate::markdown::preview(&thread.post.message).into(),
            at: thread.last_reply_at.max(thread.post.create_at),
            counts: Some((
                thread.reply_count,
                thread.unread_replies,
                thread.unread_mentions,
            )),
            target: Target::Followed(thread.id.clone()),
        })
        .collect();

    Inbox {
        mentions,
        threads,
        saved,
    }
}

// -------------------------------------------------------------------- drawing

fn inbox_row(ui: &Rc<Ui>, index: usize, row: &InboxRow, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    let mut body = v_flex().flex_1().min_w_0().gap_0p5().child(
        h_flex()
            .gap_1p5()
            .items_center()
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(row.author.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(row.channel.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(muted)
                    .child(format_relative(row.at)),
            ),
    );
    body = body.child(div().line_clamp(2).child(row.preview.clone()));

    if let Some((replies, unread_replies, unread_mentions)) = row.counts {
        let mut footer = h_flex().gap_1p5().items_center().child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!(
                    "{replies} {}",
                    message::plural(replies, "reply", "replies")
                )),
        );
        if unread_mentions > 0 {
            footer = footer.child(kit::mention_badge(unread_mentions, false, cx));
        } else if unread_replies > 0 {
            footer = footer.child(kit::with_tooltip(
                "unread",
                kit::unread_dot(cx),
                "Unread replies",
            ));
        }
        body = body.child(footer);
    }

    let target = row.target.clone();
    h_flex()
        .id(("inbox-row", index))
        .w_full()
        .items_start()
        .gap_2p5()
        .p_2()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(theme.list_hover))
        .child(kit::avatar(ui, &row.user_id, &row.author, 32.))
        .child(body)
        // The whole entry is one target.
        .on_click(ui.click(move |ui, cx| {
            ui.dispatch(
                match target.clone() {
                    Target::Thread {
                        channel_id,
                        root_id,
                    } => Action::OpenPost(channel_id, root_id),
                    Target::Message {
                        channel_id,
                        post_id,
                    } => Action::JumpToPost(channel_id, post_id),
                    Target::Followed(root_id) => Action::OpenThread(root_id),
                },
                cx,
            )
        }))
        .into_any_element()
}

fn inbox(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let panel = &ui.right;
    let inbox = panel.inbox.borrow();
    let tab = panel.tab.get();
    let (rows, empty) = match tab {
        InboxTab::Mentions => (
            &inbox.mentions,
            (
                Lucide::AtSign,
                "No recent mentions",
                "Messages that name you show up here.",
            ),
        ),
        InboxTab::Threads => (
            &inbox.threads,
            (
                Lucide::MessagesSquare,
                "No threads yet",
                "Threads you follow appear here.",
            ),
        ),
        InboxTab::Saved => (
            &inbox.saved,
            (
                Lucide::Bookmark,
                "Nothing saved",
                "Save a message from its menu and it waits here.",
            ),
        ),
    };

    let tabs = TabBar::new("inbox-tabs")
        .segmented()
        .selected_index(match tab {
            InboxTab::Mentions => 0,
            InboxTab::Threads => 1,
            InboxTab::Saved => 2,
        })
        .child(Tab::new().label("Mentions"))
        .child(Tab::new().label("Threads"))
        .child(Tab::new().label("Saved"))
        .on_click({
            let ui = ui.clone();
            move |index: &usize, _, cx| {
                ui.right.tab.set(match index {
                    1 => InboxTab::Threads,
                    2 => InboxTab::Saved,
                    _ => InboxTab::Mentions,
                });
                cx.refresh_windows();
            }
        });

    let body: AnyElement = if rows.is_empty() {
        kit::empty_state(empty.0, empty.1, empty.2, cx)
    } else {
        let mut column = v_flex().id("inbox-rows").size_full().p_1p5().gap_0p5();
        for (index, row) in rows.iter().enumerate() {
            column = column.child(inbox_row(ui, index, row, cx));
        }
        column.overflow_y_scroll().into_any_element()
    };

    v_flex()
        .flex_1()
        .min_h_0()
        .child(h_flex().flex_none().justify_center().py_2().child(tabs))
        .child(div().flex_1().min_h_0().child(body))
        .into_any_element()
}

/// Search results, newest first, each one a jump into its channel.
fn search(ui: &Rc<Ui>, cx: &mut App) -> AnyElement {
    let panel = &ui.right;
    let rows = panel.search.borrow().clone();
    let searching = panel.searching.get();
    if searching && rows.is_empty() {
        return div()
            .flex_1()
            .flex()
            .justify_center()
            .pt_6()
            .child(Spinner::new())
            .into_any_element();
    }
    if rows.is_empty() {
        return div()
            .flex_1()
            .min_h_0()
            .child(kit::empty_state(
                Lucide::Search,
                "No matches",
                "Nothing here matched that search.",
                cx,
            ))
            .into_any_element();
    }

    // The hits come a page at a time. Being within a screenful of the end of
    // what is here is what asks for the next one. How far the end is comes
    // from the frame before, so it is only believed when that frame drew
    // these same rows: on the frame a page arrives it still describes the
    // shorter list, and would ask for the page after as well.
    let scroll = &panel.search_scroll;
    let measured = panel.search_drawn.replace(rows.len()) == rows.len();
    let left = scroll.max_offset().y + scroll.offset().y;
    if panel.search_more.get() && !searching && measured && left < px(400.) {
        ui.dispatch(Action::SearchMore, cx);
    }

    let mut column = v_flex()
        .id("search-rows")
        .flex_1()
        .min_h_0()
        .py_1p5()
        .track_scroll(scroll);
    for (index, row) in rows.iter().enumerate() {
        let channel_id = row.post.channel_id.clone();
        let post_id = row.post.id.clone();
        column = column.child(
            v_flex()
                .id(("hit", index))
                .w_full()
                .child(
                    // Which channel a hit came from is also the obvious place
                    // to press to go there.
                    div()
                        .id("go")
                        .ml_3()
                        .mt_2()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                        .cursor_pointer()
                        .hover(|style| style.underline())
                        .child(format!("{}  ›", row.channel))
                        .on_click(ui.click(move |ui, cx| {
                            ui.dispatch(
                                Action::JumpToPost(channel_id.clone(), post_id.clone()),
                                cx,
                            )
                        })),
                )
                .child(message::row(
                    ui,
                    &row.post,
                    &row.body,
                    RowOptions {
                        grouped: false,
                        show_thread_footer: false,
                        highlight: false,
                    },
                    cx,
                )),
        );
    }
    if searching {
        column = column.child(
            div()
                .flex()
                .justify_center()
                .py_3()
                .child(Spinner::new().small()),
        );
    }
    column.overflow_y_scroll().into_any_element()
}

fn thread(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let panel = &ui.right;
    if !panel.thread_loaded.get() {
        return div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("Loading thread…")
            .into_any_element();
    }

    let list_state = panel.list.clone();
    let row_ui = ui.clone();
    let rows = list(list_state.clone(), move |index, _, cx| {
        let rows = row_ui.right.thread.borrow();
        match rows.get(index) {
            Some(ThreadRow::Divider(label)) => div()
                .px_3()
                .pt_2()
                .child(kit::labelled_rule(label.clone(), cx.theme().border, cx))
                .into_any_element(),
            Some(ThreadRow::Post {
                post,
                grouped,
                body,
            }) => message::row(
                &row_ui,
                post,
                body,
                RowOptions {
                    grouped: *grouped,
                    // We are already in the thread.
                    show_thread_footer: false,
                    highlight: false,
                },
                cx,
            ),
            None => div().into_any_element(),
        }
    })
    .size_full()
    .py_3();

    let mut pane = v_flex().flex_1().min_h_0().child(
        div()
            .flex_1()
            .min_h_0()
            .child(rows)
            .vertical_scrollbar(&list_state),
    );

    if let Some(composer) = panel.composer.borrow().clone() {
        pane = pane.child(
            h_flex()
                .flex_none()
                .gap_2()
                .px_3()
                .pt_1()
                .pb_3()
                .items_center()
                .child(div().flex_1().min_w_0().child(Textarea::new(&composer)))
                .child(
                    Button::new("reply")
                        .icon(Lucide::SendHorizontal)
                        .primary()
                        .tooltip("Reply  (Enter)")
                        .on_click(ui.click(|ui, cx| ui.right.submit(ui, cx))),
                ),
        );
    }
    pane.into_any_element()
}

/// Draws the panel, or nothing when it is hidden.
pub fn render(ui: &Rc<Ui>, cx: &mut App) -> Option<AnyElement> {
    let panel = &ui.right;
    let mode = panel.mode(cx);
    let (title, subtitle) = match &mode {
        PanelMode::Hidden => return None,
        PanelMode::Thread(_) => ("Thread", panel.thread_channel.borrow().clone()),
        PanelMode::Inbox => ("Inbox", String::new()),
        PanelMode::Search(terms) => ("Search", terms.clone()),
    };

    let theme = cx.theme();
    let mut header = h_flex()
        .flex_none()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .px_1()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
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

    // Following a thread is how you keep getting told about it after you stop
    // being mentioned in it, so it belongs on the thread itself.
    if matches!(mode, PanelMode::Thread(_)) {
        let following = panel.following.get();
        header = header.child(
            Button::new("follow")
                .icon(if following {
                    Lucide::BellRing
                } else {
                    Lucide::Bell
                })
                .small()
                .tooltip(if following {
                    "Following this thread"
                } else {
                    "Follow this thread"
                })
                .when(following, |button| button.primary())
                .when(!following, |button| button.ghost())
                .on_click(ui.click(move |ui, cx| {
                    ui.dispatch(Action::FollowThread(!following), cx)
                })),
        );
    }
    header = header.child(
        kit::icon_button("close-panel", Lucide::X, "Close panel")
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::CloseRightPanel, cx))),
    );

    let body = match mode {
        PanelMode::Thread(_) => thread(ui, cx),
        PanelMode::Inbox => inbox(ui, cx),
        PanelMode::Search(_) => search(ui, cx),
        PanelMode::Hidden => return None,
    };

    Some(
        v_flex()
            .id("right-panel")
            .size_full()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(header)
            .child(body)
            .into_any_element(),
    )
}

/// Builds the reply box and wires what it reports to the session.
pub(super) fn build_composer(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<TextareaState>, gpui_kit::Subscription) {
    let composer = cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(1, 6)
            .submit_on_enter(true)
            .placeholder("Reply…")
    });
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&composer, move |_, event: &InputEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            InputEvent::Change => ui.later(cx, |ui, cx| ui.right.composer_changed(ui, cx)),
            InputEvent::PressEnter { shift: false, .. } => {
                ui.later(cx, |ui, cx| ui.right.submit(ui, cx))
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

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut st = AppState::new(client, User::default(), ClientConfig::default(), false);
        for (id, name) in [("u1", "anna"), ("u2", "bob")] {
            st.users.insert(
                id.into(),
                User {
                    id: id.into(),
                    username: name.into(),
                    ..Default::default()
                },
            );
        }
        st
    }

    fn post(id: &str, user: &str, at: Millis) -> Post {
        Post {
            id: id.into(),
            user_id: user.into(),
            create_at: at,
            ..Default::default()
        }
    }

    fn keys(rows: &[ThreadRow]) -> Vec<String> {
        rows.iter().map(ThreadRow::key).collect()
    }

    #[test]
    fn the_root_leads_the_thread_even_when_a_reply_shares_its_millisecond() {
        let st = state();
        // The server listed the reply first; both carry the same timestamp.
        let posts = vec![post("reply", "u2", 1_000), post("root", "u1", 1_000)];
        assert_eq!(
            keys(&build_thread_rows(posts, "root", &st)),
            ["post:root", "divider", "post:reply"]
        );
    }

    #[test]
    fn the_divider_counts_replies_and_is_absent_without_any() {
        let st = state();
        let rows = build_thread_rows(
            vec![
                post("root", "u1", 1),
                post("a", "u2", 2),
                post("b", "u2", 3),
            ],
            "root",
            &st,
        );
        assert!(matches!(&rows[1], ThreadRow::Divider(label) if label == "2 replies"));
        // The second reply follows the first; the first follows a divider and
        // so names its author again.
        let grouped: Vec<bool> = rows
            .iter()
            .filter_map(|row| match row {
                ThreadRow::Post { grouped, .. } => Some(*grouped),
                _ => None,
            })
            .collect();
        assert_eq!(grouped, [false, false, true]);

        let alone = build_thread_rows(vec![post("root", "u1", 1)], "root", &st);
        assert_eq!(keys(&alone), ["post:root"]);
    }

    #[test]
    fn a_deleted_reply_is_neither_drawn_nor_counted() {
        let st = state();
        let mut gone = post("gone", "u2", 2);
        gone.delete_at = 5;
        let rows = build_thread_rows(
            vec![post("root", "u1", 1), gone, post("kept", "u2", 3)],
            "root",
            &st,
        );
        assert_eq!(keys(&rows), ["post:root", "divider", "post:kept"]);
        assert!(matches!(&rows[1], ThreadRow::Divider(label) if label == "1 reply"));
    }

    #[test]
    fn a_saved_post_held_twice_is_listed_once() {
        let mut st = state();
        let saved = Post {
            channel_id: "c1".into(),
            message: "**keep** this".into(),
            ..post("p1", "u1", 10)
        };
        st.saved_posts.insert("p1".into());
        st.feeds.insert(
            "c1".into(),
            crate::state::ChannelFeed::from_posts(vec![saved.clone()]),
        );
        st.threads
            .insert("p1".into(), crate::state::ChannelFeed::from_posts(vec![saved]));

        let inbox = build_inbox(&st);
        assert_eq!(inbox.saved.len(), 1);
        // A preview is read, not rendered: no Markdown markers in it.
        assert_eq!(inbox.saved[0].preview.as_ref(), "keep this");
        // Not a reply, so pressing it goes to the message.
        assert!(matches!(inbox.saved[0].target, Target::Message { .. }));
    }
}
