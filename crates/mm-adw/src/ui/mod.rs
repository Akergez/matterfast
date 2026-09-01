//! Window construction and the single place where state changes are applied.
//!
//! Widget callbacks do not mutate state directly. They push an [`Action`] onto
//! a channel, and one loop on the GTK main thread applies it. That keeps the
//! borrow of `RefCell<AppState>` short and obviously non-overlapping, which is
//! otherwise the standard way a GTK + `Rc<RefCell<…>>` app panics at runtime.
//!
//! The one exception is the profile popover, which needs the widget it anchors
//! to; it is built inline from an `Rc<Ui>` capture instead.

mod chat;
mod login;
mod message;
mod profile;
mod rhs;
mod sidebar;
pub mod sso;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::glib;
use mattermost_api::bootstrap::Bootstrap;
use mattermost_api::models::*;
use mattermost_api::ws::{Event, WebSocket, WsUpdate};
use mattermost_calls::{CallSession, CallUpdate, JoinOptions};

use crate::audio::AudioIo;
use crate::avatars::Avatars;
use crate::runtime;
use crate::state::{ActiveCall, AppState, ChannelFeed, SharedState};
use crate::video;
use chat::ChatView;
use message::MessageActions;
use rhs::{PanelMode, RightPanel};
use sidebar::{ChannelSidebar, TeamRail};

/// How many messages to pull when a channel is first opened.
const INITIAL_POSTS: u32 = 60;
/// How many mentions and threads to keep in the inbox.
const INBOX_PAGE: u32 = 25;

enum Action {
    SelectTeam(String),
    SelectChannel(String),
    Send(String),
    /// Join the current channel's call, or leave the one we are in.
    ToggleCall,
    /// Turn our own microphone on or off.
    ToggleMute,
    /// Start or stop the server-side recording.
    ToggleRecording,
    /// Start or stop sharing the screen.
    ToggleScreen,
    /// Start or stop the camera.
    ToggleCamera,
    /// Open the thread rooted at this post id in the right panel.
    OpenThread(String),
    /// Reply into the currently open thread.
    ReplyInThread(String),
    /// Show the mentions and threads inbox.
    OpenInbox,
    CloseRightPanel,
    /// Toggle our own reaction: post id, emoji name.
    ToggleReaction(String, String),
    /// Jump to a channel, and into a thread when the second field is set.
    OpenPost(String, String),
    /// Open (or create) the direct-message channel with a user.
    OpenDirectMessage(String),
}

pub fn build_window(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Mattermost")
        .default_width(1320)
        .default_height(840)
        .width_request(360)
        .height_request(400)
        .build();

    let toast_overlay = adw::ToastOverlay::new();
    window.set_content(Some(&toast_overlay));

    // A layout-only mode, so the panes can be reviewed without a server.
    if std::env::var_os("MM_ADW_DEMO").is_some() {
        start_demo(&window, &toast_overlay);
        window.present();
        return;
    }

    // Development shortcut: skip the form when credentials are in the
    // environment. Handy against a local server, and how the UI gets exercised
    // end to end without a human typing.
    if let (Ok(url), Ok(login_id), Ok(password)) = (
        std::env::var("MM_ADW_SERVER"),
        std::env::var("MM_ADW_USER"),
        std::env::var("MM_ADW_PASSWORD"),
    ) {
        if let Ok(client) = mattermost_api::Client::new(&url) {
            let window_ = window.clone();
            let toasts = toast_overlay.clone();
            runtime::spawn(
                async move {
                    client
                        .login(&login_id, &password, None)
                        .await
                        .map(|me| (client, me))
                },
                move |result| match result {
                    Ok((client, me)) => start_session(&window_, &toasts, client, me),
                    Err(e) => toasts.add_toast(adw::Toast::new(&format!("Sign-in failed: {e}"))),
                },
            );
            window.present();
            return;
        }
    }

    let show_login = {
        let window = window.clone();
        let toasts = toast_overlay.clone();
        move || {
            let view = login::build({
                let window = window.clone();
                let toasts = toasts.clone();
                move |result| {
                    // Only now is the token known to work.
                    let token = result.client.token().unwrap_or_default();
                    crate::session::save(result.client.site_url(), &token);
                    start_session(&window, &toasts, result.client, result.me);
                }
            });
            toasts.set_child(Some(&view));
        }
    };

    match crate::session::load() {
        Some((server, token)) => {
            restore_session(&window, &toast_overlay, server, token, show_login)
        }
        None => show_login(),
    }

    window.present();
}

/// Tries a stored token before showing the sign-in form. `show_login` runs if
/// the token is gone or stale — a session Mattermost has since revoked is the
/// ordinary case here, not an error worth a dialog.
fn restore_session(
    window: &adw::ApplicationWindow,
    toasts: &adw::ToastOverlay,
    server: String,
    token: String,
    show_login: impl FnOnce() + 'static,
) {
    let client = match mattermost_api::Client::new(&server) {
        Ok(c) => c,
        Err(_) => {
            crate::session::clear();
            show_login();
            return;
        }
    };
    client.set_token(token);

    let spinner = gtk::Spinner::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .width_request(32)
        .height_request(32)
        .build();
    spinner.start();
    toasts.set_child(Some(&spinner));

    let window = window.clone();
    let toasts = toasts.clone();
    runtime::spawn(
        {
            let client = client.clone();
            async move { client.me().await }
        },
        move |result| match result {
            Ok(me) => start_session(&window, &toasts, client, me),
            Err(e) => {
                // A refused token is spent; anything else (server down, no
                // network) leaves it alone so the next launch can retry.
                if !matches!(&e, mattermost_api::Error::Http(_)) {
                    crate::session::clear();
                }
                tracing::info!("stored session not usable: {e}");
                show_login();
            }
        },
    );
}

/// Replaces the login view with the main UI and kicks off the startup sequence.
fn start_session(
    window: &adw::ApplicationWindow,
    toasts: &adw::ToastOverlay,
    client: mattermost_api::Client,
    me: User,
) {
    let state: SharedState = Rc::new(RefCell::new(AppState::new(
        client,
        me,
        ClientConfig::default(),
        false,
    )));
    let ui = build_session_ui(window, toasts, state);
    bootstrap(ui);
}

/// Shows the UI filled with sample data, for looking at the layout without a
/// server. Enabled with `MM_ADW_DEMO=1`.
pub fn start_demo(window: &adw::ApplicationWindow, toasts: &adw::ToastOverlay) {
    let state = crate::demo::state();
    let ui = build_session_ui(window, toasts, state);
    ui.refresh_all();
    let initial = ui.state.borrow().current_channel.clone();
    if let Some(id) = initial {
        ui.dispatch(Action::SelectChannel(id));
    }
}

/// Builds the window contents and starts the action loop.
fn build_session_ui(
    window: &adw::ApplicationWindow,
    toasts: &adw::ToastOverlay,
    state: SharedState,
) -> Rc<Ui> {
    let (tx, rx) = async_channel::unbounded::<Action>();

    // --- pane 3: the conversation
    let chat = Rc::new(ChatView::new(
        {
            let tx = tx.clone();
            move |text| {
                let _ = tx.send_blocking(Action::Send(text));
            }
        },
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::ToggleCall);
            }
        },
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::ToggleMute);
            }
        },
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::ToggleRecording);
            }
        },
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::ToggleScreen);
            }
        },
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::ToggleCamera);
            }
        },
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::OpenInbox);
            }
        },
    ));

    // --- pane 4: thread / inbox
    let right = RightPanel::new(
        {
            let tx = tx.clone();
            move || {
                let _ = tx.send_blocking(Action::CloseRightPanel);
            }
        },
        {
            let tx = tx.clone();
            move |text| {
                let _ = tx.send_blocking(Action::ReplyInThread(text));
            }
        },
        {
            let tx = tx.clone();
            move |root| {
                let _ = tx.send_blocking(Action::OpenThread(root));
            }
        },
        {
            let tx = tx.clone();
            move |channel, root| {
                let _ = tx.send_blocking(Action::OpenPost(channel, root));
            }
        },
    );

    // The right panel overlays the conversation rather than adding a permanent
    // column: it is open only while you are reading a thread or the inbox.
    let overlay = adw::OverlaySplitView::builder()
        .content(&chat.widget)
        .sidebar(&right.widget)
        .show_sidebar(false)
        .min_sidebar_width(320.0)
        .max_sidebar_width(460.0)
        .sidebar_width_fraction(0.30)
        .build();
    overlay.set_sidebar_position(gtk::PackType::End);

    // --- pane 2: channels
    let channels = Rc::new(ChannelSidebar::new({
        let tx = tx.clone();
        move |id| {
            let _ = tx.send_blocking(Action::SelectChannel(id));
        }
    }));

    // --- pane 1: teams
    let rail = Rc::new(TeamRail::new({
        let tx = tx.clone();
        move |id| {
            let _ = tx.send_blocking(Action::SelectTeam(id));
        }
    }));

    let chat_page = adw::NavigationPage::builder()
        .title("Conversation")
        .child(&overlay)
        .build();
    let channels_page = adw::NavigationPage::builder()
        .title("Channels")
        .child(&channels.widget)
        .build();

    let inner = adw::NavigationSplitView::builder()
        .sidebar(&channels_page)
        .content(&chat_page)
        .min_sidebar_width(220.0)
        .max_sidebar_width(360.0)
        .sidebar_width_fraction(0.24)
        // When collapsed these behave like a navigation stack, and the page a
        // chat app should land on is the conversation — not the sidebar.
        .show_content(true)
        .build();

    let inner_page = adw::NavigationPage::builder()
        .title("Mattermost")
        .child(&inner)
        .build();
    let rail_page = adw::NavigationPage::builder()
        .title("Teams")
        .child(&rail.widget)
        .build();

    let outer = adw::NavigationSplitView::builder()
        .sidebar(&rail_page)
        .content(&inner_page)
        .min_sidebar_width(68.0)
        .max_sidebar_width(68.0)
        .sidebar_width_fraction(0.06)
        .show_content(true)
        .build();

    // The rail is a 68px strip when expanded, where a header bar would only be
    // an empty band; it becomes a full-width page when collapsed, where one is
    // needed for the back button.
    outer
        .bind_property("collapsed", &rail.header, "visible")
        .sync_create()
        .build();

    // Collapse in three steps: the thread panel overlays first, then the
    // conversation takes the sidebars' space, then the rail folds away.
    add_breakpoint(window, 1200.0, &[], &[(&overlay, true)]);
    add_breakpoint(window, 1000.0, &[(&inner, true)], &[(&overlay, true)]);
    add_breakpoint(
        window,
        640.0,
        &[(&inner, true), (&outer, true)],
        &[(&overlay, true)],
    );

    // Remote video floats over the panes rather than in them: a share is
    // something you glance at while carrying on reading.
    let videos = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .margin_start(12)
        .margin_bottom(12)
        .visible(false)
        .build();
    let video_overlay = gtk::Overlay::new();
    video_overlay.set_child(Some(&outer));
    video_overlay.add_overlay(&videos);

    toasts.set_child(Some(&video_overlay));

    let avatars = Avatars::new(state.borrow().client.clone());

    let ui = Rc::new(Ui {
        window: window.clone(),
        state: state.clone(),
        rail: rail.clone(),
        channels: channels.clone(),
        chat: chat.clone(),
        right: right.clone(),
        overlay: overlay.clone(),
        videos: videos.clone(),
        video_views: RefCell::new(HashMap::new()),
        outer: outer.clone(),
        inner: inner.clone(),
        avatars: avatars.clone(),
        toasts: toasts.clone(),
        tx: tx.clone(),
    });

    // A picture arriving is a reason to redraw the messages, and nothing else.
    avatars.connect_loaded({
        let ui = ui.clone();
        // The sidebar draws faces too now, so a landed texture has to repaint
        // it as well — otherwise DM rows keep their initials until the next
        // unrelated refresh.
        move || {
            ui.refresh_messages();
            ui.channels.refresh(&ui.state, &ui.avatars);
        }
    });

    glib::spawn_future_local({
        let ui = ui.clone();
        async move {
            while let Ok(action) = rx.recv().await {
                ui.handle(action);
            }
        }
    });

    ui
}

fn add_breakpoint(
    window: &adw::ApplicationWindow,
    max_width_sp: f64,
    splits: &[(&adw::NavigationSplitView, bool)],
    overlays: &[(&adw::OverlaySplitView, bool)],
) {
    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        max_width_sp,
        adw::LengthUnit::Sp,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    for (view, collapsed) in splits {
        breakpoint.add_setter(*view, "collapsed", Some(&(*collapsed).to_value()));
    }
    for (view, collapsed) in overlays {
        breakpoint.add_setter(*view, "collapsed", Some(&(*collapsed).to_value()));
    }
    window.add_breakpoint(breakpoint);
}

/// Everything the action loop needs.
struct Ui {
    window: adw::ApplicationWindow,
    state: SharedState,
    rail: Rc<TeamRail>,
    channels: Rc<ChannelSidebar>,
    chat: Rc<ChatView>,
    right: Rc<RightPanel>,
    overlay: adw::OverlaySplitView,
    /// Where remote screens and cameras are shown, keyed by the media session
    /// they belong to.
    videos: gtk::Box,
    video_views: RefCell<HashMap<String, video::RemoteView>>,
    outer: adw::NavigationSplitView,
    inner: adw::NavigationSplitView,
    avatars: Avatars,
    toasts: adw::ToastOverlay,
    tx: async_channel::Sender<Action>,
}

impl Ui {
    fn toast(&self, message: &str) {
        self.toasts.add_toast(adw::Toast::new(message));
    }

    fn dispatch(&self, action: Action) {
        let _ = self.tx.send_blocking(action);
    }

    /// The callbacks a message row needs. Rebuilt per redraw, which is cheap —
    /// they are three `Rc` clones.
    fn message_actions(self: &Rc<Self>) -> MessageActions {
        MessageActions {
            open_thread: {
                let ui = self.clone();
                Rc::new(move |root| ui.dispatch(Action::OpenThread(root)))
            },
            toggle_reaction: {
                let ui = self.clone();
                Rc::new(move |post, emoji| ui.dispatch(Action::ToggleReaction(post, emoji)))
            },
            show_profile: {
                let ui = self.clone();
                Rc::new(move |user_id, anchor| ui.show_profile(&user_id, &anchor))
            },
        }
    }

    /// Redraws only the message surfaces — used when something cosmetic lands,
    /// such as an avatar finishing its download.
    fn refresh_messages(self: &Rc<Self>) {
        let actions = self.message_actions();
        self.chat.refresh(&self.state, &self.avatars, &actions);
        self.right.refresh(&self.state, &self.avatars, &actions);
    }

    fn refresh_all(self: &Rc<Self>) {
        let (teams, current_team) = {
            let st = self.state.borrow();
            (st.teams.clone(), st.current_team.clone())
        };
        self.rail.refresh(&teams, current_team.as_deref());
        self.channels.refresh(&self.state, &self.avatars);
        self.refresh_messages();
        self.refresh_call_ui();
        self.refresh_title();
        self.hydrate_dm_teammates();
    }

    /// A DM channel carries no display name — just `"<idA>__<idB>"` — so until
    /// the other person is in `users` the sidebar row is blank. Nothing in the
    /// startup sequence fetches them, so do it here: after any load that
    /// replaces the channel list.
    fn hydrate_dm_teammates(self: &Rc<Self>) {
        let (client, ids) = {
            let st = self.state.borrow();
            let ids: Vec<String> = st
                .channels
                .values()
                .filter(|c| c.r#type == ChannelType::Direct)
                .filter_map(|c| c.dm_teammate_id(&st.me.id))
                // Missing either half is a reason to ask: a teammate known
                // from a message may still have no presence.
                .filter(|id| !st.users.contains_key(*id) || !st.statuses.contains_key(*id))
                .map(str::to_string)
                .collect();
            (st.client.clone(), ids)
        };
        if ids.is_empty() {
            return;
        }

        let ui = self.clone();
        runtime::spawn(
            async move {
                (
                    client.users_by_ids(&ids).await.unwrap_or_default(),
                    client.statuses_by_ids(&ids).await.unwrap_or_default(),
                )
            },
            move |(users, statuses)| {
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                }
                ui.channels.refresh(&ui.state, &ui.avatars);
                ui.refresh_title();
            },
        );
    }

    /// Mirrors what the desktop app puts in its tray badge: a single mention
    /// count for the whole account.
    fn refresh_title(&self) {
        let mentions = self.state.borrow().total_mentions();
        self.window.set_title(Some(&if mentions > 0 {
            format!("({mentions}) Mattermost")
        } else {
            "Mattermost".to_string()
        }));
    }

    fn refresh_call_ui(&self) {
        let st = self.state.borrow();
        let available = st.calls.is_some() && st.current_channel.is_some();
        let reason = if st.calls.is_none() {
            Some("Calls are not enabled on this server")
        } else {
            None
        };
        // The call we are in only shows on its own channel; switching away
        // leaves it running, exactly as the web client does.
        let ours = st
            .call
            .as_ref()
            .filter(|c| Some(&c.channel_id) == st.current_channel.as_ref());
        let in_call = ours.is_some();
        let muted = ours.is_none_or(|c| c.muted);
        let recording = ours.is_some_and(|c| c.recording);
        let sharing = ours.is_some_and(|c| c.screen.is_some());
        let on_camera = ours.is_some_and(|c| c.camera.is_some());
        let in_progress = st
            .current_channel
            .as_ref()
            .and_then(|id| st.active_calls.get(id))
            .copied();
        drop(st);
        self.chat.set_calls_available(available, reason);
        self.chat.set_call_in_progress(in_progress);
        self.chat.set_in_call(
            in_call,
            in_progress.is_some(),
            muted,
            recording,
            sharing,
            on_camera,
        );
    }

    fn show_profile(self: &Rc<Self>, user_id: &str, anchor: &gtk::Widget) {
        let popover = profile::popover(user_id, &self.state, &self.avatars, anchor, {
            let ui = self.clone();
            move |id| ui.dispatch(Action::OpenDirectMessage(id))
        });
        let Some(popover) = popover else {
            self.toast("That user has not loaded yet.");
            return;
        };
        // The popover parents itself onto the anchor, so it has to unparent
        // when it closes or the anchor keeps a widget it no longer shows.
        popover.connect_closed(|p| p.unparent());
        popover.popup();
    }

    fn handle(self: &Rc<Self>, action: Action) {
        match action {
            Action::SelectTeam(team_id) => self.select_team(team_id),
            Action::SelectChannel(channel_id) => self.select_channel(channel_id),
            Action::Send(text) => self.send_message(text, None),
            Action::ToggleCall => self.toggle_call(),
            Action::ToggleMute => self.toggle_mute(),
            Action::ToggleRecording => self.toggle_recording(),
            Action::ToggleScreen => self.toggle_screen(),
            Action::ToggleCamera => self.toggle_camera(),
            Action::OpenThread(root_id) => self.open_thread(root_id),
            Action::ReplyInThread(text) => {
                if let PanelMode::Thread(root) = self.right.mode() {
                    self.send_message(text, Some(root));
                }
            }
            Action::OpenInbox => self.open_inbox(),
            Action::CloseRightPanel => {
                self.right.set_mode(PanelMode::Hidden);
                self.overlay.set_show_sidebar(false);
            }
            Action::ToggleReaction(post_id, emoji) => self.toggle_reaction(post_id, emoji),
            Action::OpenPost(channel_id, root_id) => {
                self.select_channel(channel_id);
                if !root_id.is_empty() {
                    self.open_thread(root_id);
                }
            }
            Action::OpenDirectMessage(user_id) => self.open_direct_message(user_id),
        }
    }

    // ------------------------------------------------------------- navigation

    fn select_team(self: &Rc<Self>, team_id: String) {
        // On a collapsed window, picking a team should walk forward to that
        // team's channel list rather than straight into a conversation.
        self.outer.set_show_content(true);
        self.inner.set_show_content(false);
        {
            let mut st = self.state.borrow_mut();
            if st.current_team.as_deref() == Some(team_id.as_str()) {
                return;
            }
            st.current_team = Some(team_id.clone());
            st.current_channel = None;
        }

        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                tokio::try_join!(
                    client.my_channels(&team_id, false, 0),
                    client.my_channel_members(&team_id),
                    client.sidebar_categories(&team_id),
                )
            },
            move |result| match result {
                Ok((channels, members, categories)) => {
                    let first = {
                        let mut st = ui.state.borrow_mut();
                        st.channels.clear();
                        st.memberships.clear();
                        for c in channels {
                            st.channels.insert(c.id.clone(), c);
                        }
                        for m in members {
                            st.memberships.insert(m.channel_id.clone(), m);
                        }
                        st.categories = categories;
                        st.sidebar_groups()
                            .into_iter()
                            .flat_map(|(_, cs)| cs)
                            .next()
                            .map(|c| c.id)
                    };
                    ui.refresh_all();
                    ui.load_inbox();
                    if let Some(id) = first {
                        ui.dispatch(Action::SelectChannel(id));
                    }
                }
                Err(e) => ui.toast(&format!("Could not load that team: {e}")),
            },
        );
    }

    fn select_channel(self: &Rc<Self>, channel_id: String) {
        self.outer.set_show_content(true);
        self.inner.set_show_content(true);
        {
            let mut st = self.state.borrow_mut();
            if st.current_channel.as_deref() == Some(channel_id.as_str()) {
                return;
            }
            st.current_channel = Some(channel_id.clone());
        }
        self.refresh_messages();
        self.refresh_call_ui();

        let (client, crt, have_feed) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.crt_enabled,
                st.feeds.contains_key(&channel_id),
            )
        };

        if !have_feed {
            let id = channel_id.clone();
            let fetch_client = client.clone();
            runtime::spawn(
                async move {
                    let posts = fetch_client
                        .posts_for_channel(&id, 0, INITIAL_POSTS, crt)
                        .await?;
                    let (authors, statuses) = hydrate_authors(&fetch_client, &posts).await;
                    Ok::<_, mattermost_api::Error>((posts, authors, statuses))
                },
                {
                    let ui = self.clone();
                    let channel_id = channel_id.clone();
                    move |result| match result {
                        Ok((posts, authors, statuses)) => {
                            {
                                let mut st = ui.state.borrow_mut();
                                for user in authors {
                                    st.users.insert(user.id.clone(), user);
                                }
                                st.apply_statuses(statuses);
                                st.feeds
                                    .insert(channel_id.clone(), ChannelFeed::from_list(&posts));
                            }
                            ui.refresh_messages();
                            ui.chat.focus_composer();
                        }
                        Err(e) => ui.toast(&format!("Could not load messages: {e}")),
                    }
                },
            );
        } else {
            self.chat.focus_composer();
        }

        // Tell the server we are looking at this channel so read state syncs to
        // our other sessions.
        let id = channel_id.clone();
        let view_client = client.clone();
        runtime::spawn(
            async move { view_client.view_channel(&id, "", crt).await },
            |_| {},
        );

        // Locally zero the unread counters so the sidebar reacts immediately.
        {
            let mut st = self.state.borrow_mut();
            let total = st
                .channels
                .get(&channel_id)
                .map(|c| (c.total_msg_count, c.total_msg_count_root));
            if let (Some((total, total_root)), Some(member)) =
                (total, st.memberships.get_mut(&channel_id))
            {
                member.msg_count = total;
                member.msg_count_root = total_root;
                member.mention_count = 0;
                member.mention_count_root = 0;
                member.urgent_mention_count = 0;
            }
        }
        self.channels.refresh(&self.state, &self.avatars);
        self.refresh_title();
    }

    fn open_direct_message(self: &Rc<Self>, user_id: String) {
        let (client, me) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone())
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.create_direct_channel(&me, &user_id).await },
            move |result| match result {
                Ok(channel) => {
                    let id = channel.id.clone();
                    ui.state.borrow_mut().channels.insert(id.clone(), channel);
                    ui.channels.refresh(&ui.state, &ui.avatars);
                    ui.dispatch(Action::SelectChannel(id));
                }
                Err(e) => ui.toast(&format!("Could not open that conversation: {e}")),
            },
        );
    }

    // ---------------------------------------------------------------- threads

    fn open_thread(self: &Rc<Self>, root_id: String) {
        self.right.set_mode(PanelMode::Thread(root_id.clone()));
        self.overlay.set_show_sidebar(true);
        self.refresh_messages();

        // Always refetch. A thread we opened earlier may have grown, and the
        // root's reply count is not enough to tell which replies we hold.
        let (client, crt) = {
            let st = self.state.borrow();
            (st.client.clone(), st.crt_enabled)
        };
        let id = root_id.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let list = client.post_thread(&id, crt).await?;
                let (authors, statuses) = hydrate_authors(&client, &list).await;
                Ok::<_, mattermost_api::Error>((list, authors, statuses))
            },
            move |result| match result {
                Ok((list, authors, statuses)) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.threads.insert(root_id, ChannelFeed::from_list(&list));
                    }
                    ui.refresh_messages();
                    ui.right.focus_composer();
                }
                Err(e) => {
                    ui.toast(&format!("Could not load the thread: {e}"));
                    // Stop the panel claiming it is still loading. Whatever we
                    // hold of the root is better than a spinner that never
                    // resolves.
                    let root = ui.state.borrow().find_post(&root_id).cloned();
                    if let Some(root) = root {
                        ui.state.borrow_mut().threads.insert(
                            root_id,
                            ChannelFeed {
                                posts: vec![root],
                                ..Default::default()
                            },
                        );
                    }
                    ui.refresh_messages();
                }
            },
        );
    }

    fn open_inbox(self: &Rc<Self>) {
        self.right.set_mode(PanelMode::Inbox);
        self.overlay.set_show_sidebar(true);

        // Land on whichever tab has something to show: unread threads with no
        // mentions would otherwise open onto an empty list.
        let st = self.state.borrow();
        let threads_first = st.mentions.is_empty() && st.unread_threads() > 0;
        drop(st);
        if threads_first {
            self.right.show_threads_tab();
        }

        self.refresh_messages();
        self.load_inbox();
    }

    /// Fetches recent mentions and the thread inbox for the current team.
    fn load_inbox(self: &Rc<Self>) {
        let (client, team_id, username, crt) = {
            let st = self.state.borrow();
            let Some(team) = st.current_team.clone() else {
                return;
            };
            (
                st.client.clone(),
                team,
                st.me.username.clone(),
                st.crt_enabled,
            )
        };

        let ui = self.clone();
        runtime::spawn(
            async move {
                // Mattermost's "Recent Mentions" is literally a search for your
                // mention keys; @username is the one every account has.
                let mentions = client
                    .search_posts(&team_id, &format!("@{username}"), false)
                    .await
                    .ok();
                let threads = if crt {
                    client.my_threads(&team_id, false, INBOX_PAGE).await.ok()
                } else {
                    None
                };

                let mut ids: HashSet<String> = HashSet::new();
                if let Some(m) = &mentions {
                    ids.extend(m.posts.posts.values().map(|p| p.user_id.clone()));
                }
                if let Some(t) = &threads {
                    ids.extend(t.threads.iter().map(|t| t.post.user_id.clone()));
                }
                let ids: Vec<String> = ids.into_iter().filter(|i| !i.is_empty()).collect();
                let authors = if ids.is_empty() {
                    Vec::new()
                } else {
                    client.users_by_ids(&ids).await.unwrap_or_default()
                };
                (mentions, threads, authors)
            },
            move |(mentions, threads, authors)| {
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    if let Some(results) = mentions {
                        st.mentions = results.posts.ordered().cloned().collect();
                    }
                    if let Some(threads) = threads {
                        st.thread_inbox = threads.threads;
                    }
                }
                ui.refresh_messages();
            },
        );
    }

    // ---------------------------------------------------------------- posting

    fn send_message(self: &Rc<Self>, text: String, root_id: Option<String>) {
        let (client, channel_id, me) = {
            let st = self.state.borrow();
            let channel = match &root_id {
                // A reply belongs to the root's channel, which is not
                // necessarily the one on screen.
                Some(root) => st
                    .find_post(root)
                    .map(|p| p.channel_id.clone())
                    .or_else(|| st.current_channel.clone()),
                None => st.current_channel.clone(),
            };
            match channel {
                Some(id) => (st.client.clone(), id, st.me.id.clone()),
                None => return,
            }
        };

        // Show it immediately. The websocket echo replaces this copy — matched
        // on `pending_post_id` — so a slow round trip never looks like a
        // dropped message.
        let pending_id = format!("pending{}", glib::monotonic_time());
        let optimistic = Post {
            id: pending_id.clone(),
            pending_post_id: pending_id.clone(),
            channel_id: channel_id.clone(),
            user_id: me,
            message: text.clone(),
            root_id: root_id.clone().unwrap_or_default(),
            create_at: glib::real_time() / 1000,
            ..Default::default()
        };
        self.state.borrow_mut().apply_post(optimistic);
        self.refresh_messages();

        let ui = self.clone();
        let reply_to = root_id.clone();
        let placeholder_id = pending_id.clone();
        runtime::spawn(
            async move {
                let post = Post {
                    channel_id,
                    message: text,
                    root_id: reply_to.unwrap_or_default(),
                    pending_post_id: pending_id,
                    ..Default::default()
                };
                client.create_post(&post).await
            },
            move |result| match result {
                Ok(post) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        // Retire the optimistic copy explicitly rather than
                        // trusting the response to carry `pending_post_id`
                        // back — otherwise a server that drops it leaves the
                        // message on screen twice.
                        st.remove_post(&placeholder_id);
                        st.apply_post(post);
                    }
                    ui.refresh_messages();
                }
                Err(e) => {
                    ui.toast(&format!("Message not sent: {e}"));
                    // Take the optimistic copy back down: leaving it there
                    // would claim the message was sent.
                    ui.state.borrow_mut().remove_post(&placeholder_id);
                    ui.refresh_messages();
                }
            },
        );
    }

    fn toggle_reaction(self: &Rc<Self>, post_id: String, emoji: String) {
        let (client, me, currently_mine) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.has_my_reaction(&post_id, &emoji),
            )
        };

        // Optimistic, like the send path: a reaction is the one interaction
        // where a round trip is very visible.
        let reaction = Reaction {
            user_id: me.clone(),
            post_id: post_id.clone(),
            emoji_name: emoji.clone(),
            ..Default::default()
        };
        self.state
            .borrow_mut()
            .apply_reaction(&reaction, !currently_mine);
        self.refresh_messages();

        let ui = self.clone();
        let request_post = post_id.clone();
        let request_emoji = emoji.clone();
        runtime::spawn(
            async move {
                if currently_mine {
                    client
                        .remove_reaction(&me, &request_post, &request_emoji)
                        .await
                } else {
                    client
                        .add_reaction(&me, &request_post, &request_emoji)
                        .await
                        .map(|_| ())
                }
            },
            move |result| {
                if let Err(e) = result {
                    ui.toast(&format!("Reaction failed: {e}"));
                    // Put it back the way it was.
                    ui.state
                        .borrow_mut()
                        .apply_reaction(&reaction, currently_mine);
                    ui.refresh_messages();
                }
            },
        );
    }

    /// The call button: join the current channel's call, or hang up.
    fn toggle_call(self: &Rc<Self>) {
        let st = self.state.borrow();
        if let Some(call) = &st.call {
            let session = call.session.clone();
            drop(st);
            // Dropping `ActiveCall` stops the microphone; the SFU is told
            // separately, and either way we are out of the call.
            self.close_videos();
            self.state.borrow_mut().call = None;
            self.refresh_call_ui();
            runtime::spawn(async move { session.leave().await }, |_| {});
            return;
        }
        let Some(channel_id) = st.current_channel.clone() else {
            return;
        };
        let (Some(discovery), Some(ws)) = (st.calls.clone(), st.ws.clone()) else {
            drop(st);
            self.toast("Calls are not enabled on this server.");
            return;
        };
        let client = st.client.clone();
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move {
                let opts = JoinOptions {
                    channel_id,
                    ..Default::default()
                };
                CallSession::join(&client, ws, opts, &discovery).await
            },
            move |result| match result {
                Ok(session) => ui.call_joined(session),
                Err(e) => ui.toast(&format!("Could not join the call: {e}")),
            },
        );
    }

    /// Starts audio for a freshly joined call and subscribes to its updates.
    fn call_joined(self: &Rc<Self>, session: Arc<CallSession>) {
        let audio = match AudioIo::start(session.clone()) {
            Ok(audio) => audio,
            Err(e) => {
                self.toast(&format!("No audio for this call: {e}"));
                runtime::spawn(async move { session.leave().await }, |_| {});
                return;
            }
        };
        self.state.borrow_mut().call = Some(ActiveCall {
            channel_id: session.channel_id().to_string(),
            recording: false,
            roster: HashMap::new(),
            screen: None,
            camera: None,
            audio,
            // Mattermost clients join muted, and so does the session itself.
            muted: session.is_muted(),
            session: session.clone(),
        });

        let updates = session.subscribe();
        runtime::spawn_stream(
            move |tx| async move {
                let mut rx = updates;
                loop {
                    match rx.recv().await {
                        Ok(update) => {
                            if tx.send(update).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            },
            {
                let ui = self.clone();
                move |update| ui.apply_call_update(update)
            },
        );

        // The roster arrived before that subscription existed.
        if let Some(state) = session.last_state() {
            self.apply_call_update(CallUpdate::State(state));
        }
        self.refresh_call_ui();
    }

    fn toggle_mute(self: &Rc<Self>) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        let muted = call.muted;
        drop(st);

        let ui = self.clone();
        if muted {
            runtime::spawn(
                async move { session.unmute().await },
                move |result| match result {
                    // The capture loop discards frames until it has this track.
                    Ok(track) => {
                        if let Some(call) = ui.state.borrow().call.as_ref() {
                            call.audio.set_track(track);
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not unmute: {e}")),
                },
            );
        } else {
            runtime::spawn(async move { session.mute().await }, move |result| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not mute: {e}"));
                }
            });
        }
    }

    fn toggle_recording(self: &Rc<Self>) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let (client, channel_id, on) =
            (st.client.clone(), call.channel_id.clone(), !call.recording);
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move { mattermost_calls::set_recording(&client, &channel_id, on).await },
            move |result| {
                if let Err(e) = result {
                    // Only the host may record, and only when the server
                    // allows it at all; both come back as a plain refusal.
                    ui.toast(&format!("Could not change the recording: {e}"));
                }
            },
        );
    }

    fn toggle_screen(self: &Rc<Self>) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        if call.screen.is_some() {
            drop(st);
            if let Some(call) = self.state.borrow_mut().call.as_mut() {
                // Dropping the sender stops the encoder and the portal capture.
                call.screen = None;
            }
            self.refresh_call_ui();
            runtime::spawn(async move { session.stop_screen_share().await }, |_| {});
            return;
        }
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move {
                // The portal picker comes first: there is no point announcing a
                // share the user is about to cancel.
                let (node_id, fd) = video::pick_screen().await?;
                let track = session
                    .start_screen_share()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((track, node_id, fd))
            },
            move |result| {
                let sender = result.and_then(|(track, node_id, fd)| {
                    video::VideoSender::screen(track, node_id, fd)
                });
                match sender {
                    Ok(sender) => {
                        if let Some(call) = ui.state.borrow_mut().call.as_mut() {
                            call.screen = Some(sender);
                        }
                        ui.refresh_call_ui();
                    }
                    Err(e) => ui.toast(&format!("Could not share the screen: {e}")),
                }
            },
        );
    }

    fn toggle_camera(self: &Rc<Self>) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        if call.camera.is_some() {
            drop(st);
            if let Some(call) = self.state.borrow_mut().call.as_mut() {
                call.camera = None;
            }
            self.refresh_call_ui();
            runtime::spawn(async move { session.stop_video().await }, |_| {});
            return;
        }
        drop(st);

        let ui = self.clone();
        runtime::spawn(async move { session.start_video().await }, move |result| {
            let sender = result
                .map_err(|e| e.to_string())
                .and_then(video::VideoSender::camera);
            match sender {
                Ok(sender) => {
                    if let Some(call) = ui.state.borrow_mut().call.as_mut() {
                        call.camera = Some(sender);
                    }
                    ui.refresh_call_ui();
                }
                // The server allows camera only on DM channels, and only
                // when EnableVideo is on.
                Err(e) => ui.toast(&format!("Could not start the camera: {e}")),
            }
        });
    }

    /// Removes one remote picture, if it is on screen.
    fn drop_video(&self, session_id: &str, kind: &str) {
        self.video_views
            .borrow_mut()
            .remove(&format!("{session_id}:{kind}"));
        self.videos.set_visible(self.videos.first_child().is_some());
    }

    fn close_videos(&self) {
        self.video_views.borrow_mut().clear();
        self.videos.set_visible(false);
    }

    /// Who a media session belongs to, as far as the roster knows.
    fn speaker_name(&self, session_id: &str) -> String {
        let st = self.state.borrow();
        st.call
            .as_ref()
            .and_then(|call| call.roster.get(session_id))
            .and_then(|user_id| st.users.get(user_id))
            .map(|user| st.display_name(user))
            .unwrap_or_else(|| "Someone".to_string())
    }

    fn apply_call_update(self: &Rc<Self>, update: CallUpdate) {
        if self.state.borrow().call.is_none() {
            // We hung up; whatever is still arriving belongs to a call that is
            // no longer ours.
            return;
        }
        match update {
            CallUpdate::RemoteTrack {
                session_id,
                track_type,
                track,
            } => {
                use mattermost_calls::protocol::track_type as tt;
                match track_type.as_str() {
                    // Screen audio is audio like any other; it just happens to
                    // come from a share rather than a microphone.
                    tt::VOICE | tt::SCREEN_AUDIO => {
                        if let Some(call) = self.state.borrow().call.as_ref() {
                            call.audio.play(session_id, track);
                        }
                    }
                    tt::SCREEN | tt::VIDEO => {
                        let who = self.speaker_name(&session_id);
                        let title = if track_type == tt::SCREEN {
                            format!("{who} is sharing a screen")
                        } else {
                            format!("{who} — camera")
                        };
                        match video::show_remote(&self.videos, &title, track) {
                            Ok(view) => {
                                self.video_views
                                    .borrow_mut()
                                    .insert(format!("{session_id}:{track_type}"), view);
                            }
                            Err(e) => self.toast(&format!("Could not show the video: {e}")),
                        }
                    }
                    other => tracing::debug!(track = other, "ignoring a remote track"),
                }
            }
            CallUpdate::MuteChanged { muted } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.muted = muted;
                }
                self.refresh_call_ui();
            }
            CallUpdate::State(state) => {
                let mut st = self.state.borrow_mut();
                if let Some(call) = st.call.as_mut() {
                    call.recording = is_running(state.recording.as_ref());
                    call.roster = state
                        .sessions
                        .iter()
                        .map(|s| (s.session_id.clone(), s.user_id.clone()))
                        .collect();
                    let (channel, count) = (call.channel_id.clone(), state.participant_count());
                    st.active_calls.insert(channel, count);
                }
                drop(st);
                self.refresh_call_ui();
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserScreenShare {
                session_id,
                sharing: false,
                ..
            }) => self.drop_video(&session_id, mattermost_calls::protocol::track_type::SCREEN),
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserVideo {
                session_id,
                on: false,
                ..
            }) => self.drop_video(&session_id, mattermost_calls::protocol::track_type::VIDEO),
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserLeft {
                session_id, ..
            }) => {
                use mattermost_calls::protocol::track_type as tt;
                self.drop_video(&session_id, tt::SCREEN);
                self.drop_video(&session_id, tt::VIDEO);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::JobState { state, .. }) => {
                if state.job_type != "recording" {
                    return;
                }
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.recording = is_running(Some(&state));
                }
                self.refresh_call_ui();
            }
            CallUpdate::Connected(false) | CallUpdate::Ended => {
                self.close_videos();
                self.state.borrow_mut().call = None;
                self.refresh_call_ui();
                self.toast("The call ended.");
            }
            CallUpdate::Error(e) => self.toast(&format!("Call error: {e}")),
            _ => {}
        }
    }

    // ------------------------------------------------------------ ws handling

    fn apply_event(self: &Rc<Self>, event: Event) {
        // Calls traffic arrives as plugin-namespaced events and never overlaps
        // with the core event set, so it is cheapest to split it off first.
        if let Some(calls) = mattermost_calls::signaling::parse(&event) {
            self.apply_calls_event(calls);
            return;
        }

        let mut redraw_messages = false;
        let mut redraw_sidebar = false;
        let mut forget_avatar: Option<String> = None;

        {
            let mut st = self.state.borrow_mut();
            let me = st.me.id.clone();
            let crt = st.crt_enabled;

            match event {
                Event::Posted(posted) => {
                    let post = posted.post;
                    let channel_id = post.channel_id.clone();
                    let is_mine = post.user_id == me;
                    let mentions_me = posted.mentions.contains(&me);
                    let viewing = st.current_channel.as_deref() == Some(channel_id.as_str());
                    let is_reply = post.is_reply();
                    let create_at = post.create_at;

                    st.apply_post(post.clone());

                    // The channel's own counter has to move too, or the unread
                    // maths (total − my count) goes negative on the next read.
                    if let Some(channel) = st.channels.get_mut(&channel_id) {
                        channel.total_msg_count += 1;
                        if !is_reply {
                            channel.total_msg_count_root += 1;
                        }
                        channel.last_post_at = create_at;
                    }
                    if let Some(member) = st.memberships.get_mut(&channel_id) {
                        if is_mine || viewing {
                            // Reading it counts as reading it.
                            member.msg_count += 1;
                            if !is_reply {
                                member.msg_count_root += 1;
                            }
                        } else if mentions_me {
                            member.mention_count += 1;
                            if !is_reply || !crt {
                                member.mention_count_root += 1;
                            }
                        }
                    }
                    if mentions_me && !is_mine {
                        st.mentions.insert(0, post);
                        st.mentions.truncate(INBOX_PAGE as usize);
                    }
                    redraw_messages = true;
                    redraw_sidebar = true;
                }
                Event::PostEdited(post) => {
                    st.apply_post(post);
                    redraw_messages = true;
                }
                Event::PostDeleted(post) => {
                    if let Some(feed) = st.feeds.get_mut(&post.channel_id) {
                        feed.remove(&post.id);
                    }
                    let root = post.thread_root().to_string();
                    if let Some(thread) = st.threads.get_mut(&root) {
                        thread.remove(&post.id);
                    }
                    redraw_messages = true;
                }
                Event::ReactionAdded(reaction) => {
                    st.apply_reaction(&reaction, true);
                    redraw_messages = true;
                }
                Event::ReactionRemoved(reaction) => {
                    st.apply_reaction(&reaction, false);
                    redraw_messages = true;
                }
                Event::UserUpdated(user) => {
                    // A new picture means the cached texture is stale.
                    let changed = st
                        .users
                        .get(&user.id)
                        .map(|old| old.last_picture_update != user.last_picture_update)
                        .unwrap_or(true);
                    if changed {
                        forget_avatar = Some(user.id.clone());
                    }
                    st.users.insert(user.id.clone(), *user);
                    redraw_messages = true;
                }
                Event::StatusChange { user_id, status } => {
                    let presence = Presence::from(status.as_str());
                    // Only redraw when the dot would actually change colour —
                    // the server repeats a user's current status often enough
                    // that rebuilding the feed each time would be noticeable.
                    // Unknown users render as offline already, hence the
                    // default rather than an Option compare.
                    if st.statuses.insert(user_id, presence).unwrap_or_default() != presence {
                        redraw_messages = true;
                    }
                }
                Event::ChannelsViewed { channel_times } => {
                    // Another session read something; mirror it.
                    for (channel_id, at) in channel_times {
                        let totals = st
                            .channels
                            .get(&channel_id)
                            .map(|c| (c.total_msg_count, c.total_msg_count_root));
                        if let (Some((total, root)), Some(member)) =
                            (totals, st.memberships.get_mut(&channel_id))
                        {
                            member.last_viewed_at = Some(at);
                            member.msg_count = total;
                            member.msg_count_root = root;
                            member.mention_count = 0;
                            member.mention_count_root = 0;
                        }
                    }
                    redraw_sidebar = true;
                }
                _ => {}
            }
        }

        if let Some(user_id) = forget_avatar {
            self.avatars.forget(&user_id);
        }
        if redraw_messages {
            self.refresh_messages();
        }
        if redraw_sidebar {
            self.channels.refresh(&self.state, &self.avatars);
            self.refresh_title();
        }
    }

    /// Keeps the sidebar's "call in progress" marker and the channel banner
    /// honest. Joining is a separate, explicit action.
    fn apply_calls_event(self: &Rc<Self>, event: mattermost_calls::CallsEvent) {
        use mattermost_calls::CallsEvent as Ev;
        let mut touched = true;
        {
            let mut st = self.state.borrow_mut();
            match event {
                Ev::CallStarted { channel_id, .. } => {
                    st.active_calls.entry(channel_id).or_insert(0);
                }
                Ev::CallEnded { channel_id } => {
                    st.active_calls.remove(&channel_id);
                }
                // The server only sends a full roster to the joiner, so the
                // count has to follow the individual comings and goings too —
                // otherwise the banner keeps claiming a call we have left.
                Ev::UserJoined { channel_id, .. } => {
                    *st.active_calls.entry(channel_id).or_insert(0) += 1;
                }
                Ev::UserLeft { channel_id, .. } => {
                    if let Some(count) = st.active_calls.get_mut(&channel_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            st.active_calls.remove(&channel_id);
                        }
                    }
                }
                Ev::CallState { channel_id, state } => {
                    let count = state.participant_count();
                    if count > 0 {
                        st.active_calls.insert(channel_id, count);
                    } else {
                        st.active_calls.remove(&channel_id);
                    }
                }
                _ => touched = false,
            }
        }
        if touched {
            self.channels.refresh(&self.state, &self.avatars);
            self.refresh_call_ui();
        }
    }
}

/// Whether a recording/transcription job is actually running right now.
fn is_running(job: Option<&mattermost_calls::protocol::JobState>) -> bool {
    job.is_some_and(|j| j.start_at > 0 && j.end_at == 0 && j.err.is_empty())
}

/// Post authors are not included in a feed response, so hydrate them before
/// rendering or every message reads "unknown". Presence comes along for the
/// ride: the same ids, and the avatars carry a status badge.
async fn hydrate_authors(
    client: &mattermost_api::Client,
    list: &PostList,
) -> (Vec<User>, Vec<Status>) {
    let ids: Vec<String> = list
        .posts
        .values()
        .map(|p| p.user_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return (Vec::new(), Vec::new());
    }
    (
        client.users_by_ids(&ids).await.unwrap_or_default(),
        client.statuses_by_ids(&ids).await.unwrap_or_default(),
    )
}

/// Runs the startup sequence and connects the websocket.
fn bootstrap(ui: Rc<Ui>) {
    let client = ui.state.borrow().client.clone();

    runtime::spawn(
        async move {
            let boot = Bootstrap::run(&client, None, None, 0).await?;
            // Calls discovery is optional: the plugin may not be installed.
            let calls = mattermost_calls::discover(&client)
                .await
                .inspect_err(|e| tracing::warn!(error = %e, "calls discovery failed"))
                .ok();
            let active = match &calls {
                Some(_) => mattermost_calls::config::all_channel_states(&client)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            Ok::<_, mattermost_api::Error>((boot, calls, active))
        },
        move |result| {
            let (boot, calls, active) = match result {
                Ok(v) => v,
                Err(e) => {
                    ui.toast(&format!("Could not load your account: {e}"));
                    return;
                }
            };

            let ws_url;
            let token;
            let initial_channel;
            {
                let mut st = ui.state.borrow_mut();
                st.config = boot.config;
                st.crt_enabled = boot.crt_enabled;
                st.me = boot.me;
                st.teams = boot.teams;
                st.current_team = boot.initial_team.as_ref().map(|t| t.id.clone());
                for c in boot.channels {
                    st.channels.insert(c.id.clone(), c);
                }
                for m in boot.channel_members {
                    st.memberships.insert(m.channel_id.clone(), m);
                }
                st.categories = boot.categories;
                st.calls = calls;
                st.active_calls = active
                    .into_iter()
                    .filter_map(|c| {
                        let call = c.call?;
                        Some((c.channel_id, call.participant_count()))
                    })
                    .collect();

                ws_url = st.client.websocket_url();
                token = st.client.token().unwrap_or_default();
                initial_channel = boot.initial_channel.map(|c| c.id);
            }

            ui.refresh_all();
            ui.load_inbox();

            if let Some(id) = initial_channel {
                ui.dispatch(Action::SelectChannel(id));
            }

            // --- live updates
            // Inside the runtime guard: `connect` spawns its own task, and
            // calling that from a GTK callback panics with "no reactor running".
            let ws = runtime::with_runtime(|| WebSocket::connect(ws_url, token));
            ui.state.borrow_mut().ws = Some(ws.clone());

            let subscription = ws.subscribe();
            runtime::spawn_stream(
                move |tx| async move {
                    let mut rx = subscription;
                    loop {
                        match rx.recv().await {
                            Ok(update) => {
                                if tx.send(update).await.is_err() {
                                    break; // the window went away
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                },
                {
                    let ui = ui.clone();
                    move |update: WsUpdate| match update {
                        WsUpdate::Event(event) => ui.apply_event(event),
                        WsUpdate::MissedMessages => {
                            // The server could not replay its buffer, so
                            // anything could have changed while we were away.
                            ui.toast("Reconnected — refreshing.");
                            resync(&ui);
                        }
                        WsUpdate::Disconnected {
                            will_retry: false,
                            reason,
                        } => ui.toast(&format!("Disconnected: {reason}")),
                        _ => {}
                    }
                },
            );
        },
    );
}

/// The gap-fill after a failed websocket resume.
fn resync(ui: &Rc<Ui>) {
    let (client, team_id, crt, channel_id, since) = {
        let st = ui.state.borrow();
        let Some(team) = st.current_team.clone() else {
            return;
        };
        let channel = st.current_channel.clone();
        let since = channel
            .as_ref()
            .and_then(|id| st.feeds.get(id))
            .map(|f| f.last_fetched_at)
            .unwrap_or(0);
        (st.client.clone(), team, st.crt_enabled, channel, since)
    };

    let ui = ui.clone();
    runtime::spawn(
        async move {
            let channels = client.my_channels(&team_id, false, 0).await?;
            let members = client.my_channel_members(&team_id).await?;
            let missed = match (&channel_id, since) {
                (Some(id), since) if since > 0 => Some(client.posts_since(id, since, crt).await?),
                _ => None,
            };
            Ok::<_, mattermost_api::Error>((channels, members, channel_id, missed))
        },
        move |result| {
            let Ok((channels, members, channel_id, missed)) = result else {
                return;
            };
            {
                let mut st = ui.state.borrow_mut();
                for c in channels {
                    st.channels.insert(c.id.clone(), c);
                }
                for m in members {
                    st.memberships.insert(m.channel_id.clone(), m);
                }
                if let (Some(id), Some(list)) = (channel_id, missed) {
                    let posts: Vec<Post> = list.chronological().into_iter().cloned().collect();
                    for post in posts {
                        if post.is_deleted() {
                            if let Some(feed) = st.feeds.get_mut(&id) {
                                feed.remove(&post.id);
                            }
                        } else {
                            // `?since=` reports edits and deletes as well as
                            // new posts, so upsert rather than append.
                            st.apply_post(post);
                        }
                    }
                }
            }
            ui.refresh_all();
            ui.load_inbox();
        },
    );
}
