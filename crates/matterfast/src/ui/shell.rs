//! The window, and what is in it at each stage of a launch.
//!
//! The window is a three-pane layout — channel sidebar, conversation, and a
//! thread/inbox panel. It collapses in two steps as it narrows: the thread
//! panel is laid over the conversation first, then the channel list becomes a
//! page of its own that the conversation sits in front of. Under the sidebar
//! is the call dock, which is pinned there for as long as a call runs and
//! moves under the conversation once the sidebar is a separate page.
//!
//! The layout is read off the window's width on every frame, which is the
//! only thing that is true when a window is tiled, maximized or dragged onto
//! a phone-sized screen. The one thing kept is how wide the two side columns
//! were dragged (`Widths`), and that is a wish the window's width overrules.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use gpui_kit::component::button::Button;
use gpui_kit::component::input::Escape;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Selectable, Sizable, TitleBar, WindowExt,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, div, img, px, size, AnyElement, AnyWindowHandle, App, Bounds, Context, DispatchPhase,
    DragMoveEvent, Entity, FocusHandle, FontWeight, Global, Image, ImageFormat, KeyBinding,
    MouseButton, ObjectFit, Pixels, ScrollWheelEvent, Subscription, Window, WindowBounds,
    WindowOptions,
};
use mattermost_api::models::{ClientConfig, User};

use super::kit::{self, Lucide};
use super::login::{LoginResult, LoginView};
use super::rhs::PanelMode;
use super::{Action, Divider, MenuAction, Ui};
use crate::background::Background;
use crate::ipc::Request;
use crate::notifications::Notifier;
use crate::runtime;
use crate::state::AppState;

gpui_kit::actions!(
    matterfast,
    [
        Quit,
        QuickSwitch,
        FocusSearch,
        OpenInbox,
        ClosePanel,
        NewChannel,
        NextUnread,
        PreviousUnread,
        OpenSettings
    ]
);

/// The key context the shortcuts below live in.
const CONTEXT: &str = "Matterfast";

/// Below this the channel list is a page of its own rather than a column.
const SIDEBAR_PAGE_BELOW: f32 = 700.0;

/// Below this the thread panel has to overlay rather than take a column.
const STATIC_PANEL_MIN_WIDTH: f32 = 1200.0;

/// What the application holds for as long as it runs.
struct Matterfast {
    /// The session, once there is one. It outlives the window.
    ui: Option<Rc<Ui>>,
    window: Option<(AnyWindowHandle, Entity<Shell>)>,
    notifier: Notifier,
}

impl Global for Matterfast {}

/// The session, if somebody is signed in.
pub fn current(cx: &App) -> Option<Rc<Ui>> {
    cx.try_global::<Matterfast>().and_then(|app| app.ui.clone())
}

/// Sets up everything that outlives a window: the notification service, the
/// shortcuts, and what happens when the last window goes away.
pub fn init(cx: &mut App) {
    let (notifier, responses) = Notifier::start();
    cx.set_global(Matterfast {
        ui: None,
        window: None,
        notifier,
    });

    // Somebody pressed one of our notifications.
    runtime::receive(responses, |response, cx| {
        if let Some(ui) = current(cx) {
            ui.notification_pressed(&response.tag, response.action.as_deref(), cx);
        }
        true
    });

    // The shortcuts a chat client is expected to have. Anything reachable
    // only by mouse is reachable only slowly.
    cx.bind_keys([
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-k", QuickSwitch, Some(CONTEXT)),
        KeyBinding::new("ctrl-f", FocusSearch, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-i", OpenInbox, Some(CONTEXT)),
        KeyBinding::new("escape", ClosePanel, Some(CONTEXT)),
        KeyBinding::new("ctrl-n", NewChannel, Some(CONTEXT)),
        KeyBinding::new("ctrl-,", OpenSettings, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-down", NextUnread, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-up", PreviousUnread, Some(CONTEXT)),
    ]);
    // Closing the window does not quit, so something else has to.
    cx.on_action(|_: &Quit, cx| cx.quit());

    cx.on_window_closed(|cx, _| {
        let Some(app) = cx.try_global::<Matterfast>() else {
            return;
        };
        let (ui, window) = (app.ui.clone(), app.window.clone());
        // Ours, or some other window — a file dialog, say?
        if window.is_some_and(|(handle, _)| cx.windows().contains(&handle)) {
            return;
        }
        cx.global_mut::<Matterfast>().window = None;
        match ui {
            // The session carries on with nothing on screen: the socket stays
            // connected and notifications keep arriving. Presenting it again
            // builds a new window around the same session.
            Some(ui) if Background::enabled() => {
                ui.detach(cx);
                tracing::info!("last window closed; staying in the background");
            }
            _ => cx.quit(),
        }
    })
    .detach();
}

/// Does what a launch asked for — this one, or a later one that found us
/// already running.
pub fn handle_request(request: Request, cx: &mut App) {
    // Raising the application has to *show* something, and a callback from
    // the browser still needs a window to land in.
    present(cx);
    if let Request::Open(uri) = request {
        super::sso::deliver(&uri, cx);
    }
}

/// Shows the window: the one that is there, or a new one around whatever
/// session is running. An existing window is raised rather than duplicated —
/// a second window would mean a second session and a second websocket.
pub fn present(cx: &mut App) {
    let existing = cx
        .try_global::<Matterfast>()
        .and_then(|app| app.window.as_ref().map(|(handle, _)| *handle));
    match existing {
        Some(handle) if cx.windows().contains(&handle) => {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        _ => open_window(cx),
    }
}

/// `MATTERFAST_SIZE=400x800` opens at a phone-sized window, which is the only
/// practical way to look at the collapsed layout without a phone.
fn initial_size() -> gpui_kit::Size<Pixels> {
    let (width, height) = std::env::var("MATTERFAST_SIZE")
        .ok()
        .and_then(|size| parse_size(&size))
        .unwrap_or((1320.0, 840.0));
    size(px(width), px(height))
}

fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (width, height) = text.split_once('x')?;
    let (width, height): (f32, f32) = (width.trim().parse().ok()?, height.trim().parse().ok()?);
    (width > 0.0 && height > 0.0).then_some((width, height))
}

pub fn open_window(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            initial_size(),
            cx,
        ))),
        window_min_size: Some(size(px(360.), px(400.))),
        app_id: Some(crate::APP_ID.to_string()),
        ..TitleBar::window_options()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(window, cx))
    }) {
        Ok((handle, shell)) => {
            cx.global_mut::<Matterfast>().window = Some((handle, shell.clone()));
            let _ = handle.update(cx, |_, window, cx| {
                window.set_window_title("Matterfast");
                shell.update(cx, |shell, cx| shell.start(window, cx));
            });
            super::script::play(handle, cx);
        }
        Err(error) => tracing::error!(%error, "could not open a window"),
    }
}

/// Runs `f` with the window and what is in it, if there is one.
fn with_shell(cx: &mut App, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)) {
    let Some((handle, shell)) = cx.try_global::<Matterfast>().and_then(|app| app.window.clone())
    else {
        return;
    };
    let _ = handle.update(cx, |_, window, cx| {
        shell.update(cx, |shell, cx| f(shell, window, cx));
    });
}

/// The account was signed out: the session ends, and the window goes back to
/// the sign-in form. Any other server stays signed in, for the next launch.
pub fn signed_out(cx: &mut App) {
    if let Some(ui) = cx.global_mut::<Matterfast>().ui.take() {
        ui.detach(cx);
    }
    cx.global::<Matterfast>().notifier.session(false);
    with_shell(cx, |shell, window, cx| shell.show_login(window, cx));
}

/// A server URL with the scheme stripped, which is how people say it.
fn pretty_server(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

/// The app's own icon — the same file the desktop shows in its launcher.
fn app_icon() -> Arc<Image> {
    static ICON: OnceLock<Arc<Image>> = OnceLock::new();
    ICON.get_or_init(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../../../../data/icons/hicolor/scalable/apps/io.gitlab.akergez.Matterfast.svg")
                .to_vec(),
        ))
    })
    .clone()
}

/// What the window is showing.
enum Stage {
    /// Waiting on the keyring, or on the first answer from a server.
    Loading,
    Login(Entity<LoginView>),
    /// More than one account is stored, so ask rather than guessing which
    /// one this launch is for. Each is (server, token).
    ChooseServer(Vec<(String, String)>),
    Session(Rc<Ui>),
}

pub struct Shell {
    stage: Stage,
    focus: FocusHandle,
    /// What a sign-in failure during startup has to say; shown on the form.
    notice: RefCell<Option<String>>,
    /// The composers and the search box of the current session, kept alive
    /// for as long as this window shows it.
    _session_subscriptions: Vec<Subscription>,
    /// The desktop switching between light and dark, which a "System" theme
    /// has to follow while the window is open.
    _appearance: Subscription,
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Only now is there a window to ask what the desktop looks like.
        crate::appearance::apply(Some(window), cx);
        let appearance = window
            .observe_window_appearance(|window, cx| crate::appearance::apply(Some(window), cx));
        // Android's window announces a change of appearance only when it
        // differs from what the window itself believes, and it starts out
        // believing light, so a switch back to light goes unannounced. Every
        // change of configuration is announced as a change of keyboard layout,
        // though — from inside the platform's own lock, hence the task.
        #[cfg(target_os = "android")]
        cx.on_keyboard_layout_change(|cx| {
            cx.spawn(async move |cx| {
                let _ = cx.update(|cx| {
                    if crate::appearance::behind_the_system(cx) {
                        crate::appearance::apply(None, cx);
                        cx.refresh_windows();
                    }
                });
            })
            .detach();
        })
        .detach();
        Shell {
            _appearance: appearance,
            stage: Stage::Loading,
            focus: cx.focus_handle(),
            notice: RefCell::new(None),
            _session_subscriptions: Vec::new(),
        }
    }

    /// Decides what this window is for. Runs once, right after it opens.
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A session is already running: the window was closed and has been
        // asked for again.
        if let Some(ui) = current(cx) {
            self.show_session(ui, window, cx);
            return;
        }

        // A layout-only mode, so the panes can be reviewed without a server.
        if std::env::var_os("MATTERFAST_DEMO").is_some() {
            let ui = self.adopt(crate::demo::state(), window, cx);
            ui.later(cx, |ui, cx| {
                ui.refresh_all(cx);
                let initial = ui.state.borrow().current_channel.clone();
                if let Some(id) = initial {
                    // Selecting it again would be a no-op, so start from
                    // nowhere and let the ordinary path draw it.
                    ui.state.borrow_mut().current_channel = None;
                    ui.dispatch(Action::SelectChannel(id), cx);
                }
            });
            return;
        }

        // Development shortcut: skip the form when credentials are in the
        // environment. Handy against a local server, and how the UI gets
        // exercised end to end without a human typing.
        if let (Ok(url), Ok(login_id), Ok(password)) = (
            std::env::var("MATTERFAST_SERVER"),
            std::env::var("MATTERFAST_USER"),
            std::env::var("MATTERFAST_PASSWORD"),
        ) {
            if let Ok(client) = mattermost_api::Client::new(&url) {
                runtime::spawn(
                    async move {
                        client
                            .login(&login_id, &password, None)
                            .await
                            .map(|me| (client, me))
                    },
                    |result, cx| {
                        with_shell(cx, |shell, window, cx| match result {
                            Ok((client, me)) => shell.start_session(client, me, window, cx),
                            Err(e) => {
                                *shell.notice.borrow_mut() =
                                    Some(format!("Sign-in failed: {e}"));
                                shell.show_login(window, cx);
                            }
                        })
                    },
                );
                return;
            }
        }

        // Reading the keyring can prompt for an unlock, so the window goes up
        // first and the stored session arrives into it.
        runtime::spawn(crate::session::load_all_async(), |stored, cx| {
            with_shell(cx, |shell, window, cx| match stored.as_slice() {
                [] => shell.show_login(window, cx),
                [(server, token)] => {
                    shell.restore_session(server.clone(), token.clone(), window, cx)
                }
                _ => {
                    shell.stage = Stage::ChooseServer(stored);
                    cx.notify();
                }
            });
        });
    }

    fn show_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._session_subscriptions.clear();
        let view = cx.new(|cx| {
            LoginView::new(
                |result: LoginResult, cx| {
                    // Only now is the token known to work.
                    let token = result.client.token().unwrap_or_default();
                    let server = result.client.site_url().to_string();
                    runtime::spawn(
                        async move { crate::session::save_async(&server, &token).await },
                        |_, _| {},
                    );
                    with_shell(cx, |shell, window, cx| {
                        shell.start_session(result.client, result.me, window, cx)
                    });
                },
                window,
                cx,
            )
        });
        self.stage = Stage::Login(view);
        window.set_window_title("Matterfast");
        cx.notify();
    }

    /// Makes a session out of a state and puts it in this window.
    fn adopt(
        &mut self,
        state: crate::state::SharedState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Rc<Ui> {
        let notifier = cx.global::<Matterfast>().notifier.clone();
        notifier.session(true);
        let ui = Ui::new(state, notifier);
        cx.global_mut::<Matterfast>().ui = Some(ui.clone());
        self.show_session(ui.clone(), window, cx);
        ui
    }

    /// Puts a session on screen: builds the parts of it that only exist in a
    /// window, and hands them over.
    fn show_session(&mut self, ui: Rc<Ui>, window: &mut Window, cx: &mut Context<Self>) {
        let (composer, composer_events) = super::chat::build_composer(&ui, window, cx);
        let (reply, reply_events) = super::rhs::build_composer(&ui, window, cx);
        let (search, search_events) = super::search::build(&ui, window, cx);
        ui.chat.attach(composer);
        ui.right.attach(reply);
        ui.search_box.attach(search);
        self._session_subscriptions = vec![composer_events, reply_events, search_events];
        self.stage = Stage::Session(ui.clone());
        cx.notify();

        // The rest wants the window, and this is the window in the middle of
        // being set up.
        let handle = window.window_handle();
        ui.later(cx, move |ui, cx| ui.attach(handle, cx));
    }

    /// Replaces the login view with the main UI and kicks off the startup
    /// sequence.
    fn start_session(
        &mut self,
        client: mattermost_api::Client,
        me: User,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = Rc::new(RefCell::new(AppState::new(
            client,
            me,
            ClientConfig::default(),
            false,
        )));
        let ui = self.adopt(state, window, cx);
        // The account here was already proven live (a fresh login, or the
        // dev-shortcut password login), so there is no stale token to fall
        // back from — unlike `restore_session`'s untested one.
        ui.later(cx, |ui, cx| super::bootstrap(ui.clone(), None, cx));
    }

    /// Tries a stored token before showing the sign-in form. The form is
    /// shown if the token is gone or stale — a session Mattermost has since
    /// revoked is the ordinary case here, not an error worth a dialog.
    fn restore_session(
        &mut self,
        server: String,
        token: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let client = match mattermost_api::Client::new(&server) {
            Ok(client) => client,
            Err(_) => {
                runtime::spawn(crate::session::clear_async(), |_, _| {});
                crate::cache::clear();
                self.show_login(window, cx);
                return;
            }
        };
        client.set_token(token);

        // No round trip before this. A token read back from the keyring used
        // to be checked with its own `client.me()` call before anything was
        // built — which meant a network reply gated the very first pixel of a
        // channel list that the local store already had sitting on disk.
        // Trusting the token on sight and building the session straight away
        // lets `bootstrap`'s cache read draw into it before the network has
        // said a word. If the token turns out to be no good, `bootstrap`'s
        // own account fetch is what finds that out, and the fallback below
        // undoes the guess.
        let state = Rc::new(RefCell::new(AppState::new(
            client,
            User::default(),
            ClientConfig::default(),
            false,
        )));
        let ui = self.adopt(state, window, cx);
        ui.later(cx, |ui, cx| {
            super::bootstrap(ui.clone(), Some(Box::new(signed_out)), cx)
        });
    }

    /// The session, when that is what the window is showing.
    fn session(&self) -> Option<&Rc<Ui>> {
        match &self.stage {
            Stage::Session(ui) => Some(ui),
            _ => None,
        }
    }

    fn menu(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        if let Some(ui) = self.session() {
            ui.later(cx, move |ui, cx| ui.menu_action(action, cx));
        }
    }

    /// Escape, when nothing more specific wanted it: puts away whatever is
    /// in front. Answers whether there was anything to put away.
    fn close_front(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ui) = self.session().cloned() else {
            return false;
        };
        if super::lightbox::dismiss(&ui, cx) {
            return true;
        }
        if ui.overlay.shown() {
            ui.dispatch(Action::CloseRightPanel, cx);
            return true;
        }
        false
    }

    /// The window's own header: what this is and which server it is talking
    /// to on the left, the way to anywhere in the middle, what is waiting for
    /// you on the right. The bar under all of it is what drags the window, so
    /// everything that takes a click keeps the press to itself.
    ///
    /// It is painted as the channel sidebar is: the two are one frame around
    /// the conversation, in one colour, and everything else is the page.
    fn title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let width = f32::from(window.viewport_size().width);
        let roomy = width >= SIDEBAR_PAGE_BELOW;
        // A window that is not the one being typed into says so quietly.
        let active = window.is_window_active();

        let mut name = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(img(app_icon()).flex_none().size(px(18.)))
            .child(
                div()
                    .flex_none()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(!active, |name| name.text_color(theme.muted_foreground))
                    .child("Matterfast"),
            );
        let mut actions = h_flex()
            .flex_1()
            .min_w_0()
            .pr_1()
            .gap_1()
            .items_center()
            .justify_end();
        let mut bar = h_flex().size_full().gap_3().items_center();

        // The toolkit's bar is a gradient of its own colour; a flat fill in
        // the sidebar's is what makes the two read as one surface.
        #[cfg(not(target_os = "android"))]
        let framed = || {
            TitleBar::new()
                .bg(theme.sidebar)
                .border_color(theme.sidebar_border)
                .text_color(theme.sidebar_foreground)
        };
        // A phone has no window to drag, minimise or close, and the toolkit's
        // bar draws the buttons for that regardless: a plain strip instead,
        // tall enough for a finger.
        #[cfg(target_os = "android")]
        let framed = || {
            h_flex()
                .flex_none()
                .h(px(44.))
                .px_3()
                .border_b_1()
                .bg(theme.sidebar)
                .border_color(theme.sidebar_border)
                .text_color(theme.sidebar_foreground)
        };

        let Some(ui) = self.session().cloned() else {
            return framed().child(bar.child(name)).into_any_element();
        };
        let (server, mentions) = {
            let st = ui.state.borrow();
            (pretty_server(st.client.site_url()), st.total_mentions())
        };
        if roomy {
            name = name.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(server),
            );
        }

        // Searching messages is the one thing done from anywhere, so it has
        // the middle of the bar; going to a channel is done from the list of
        // channels, and its button is there.
        let search =
            super::search::render(&ui, column_width(width, 0.32, 150.0, 380.0), window, cx);

        let inbox_open =
            ui.overlay.shown() && matches!(ui.right.mode(cx), PanelMode::Inbox);
        actions = actions.child(
            h_flex()
                .id("title-inbox")
                .flex_none()
                .gap_1()
                .items_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when(mentions > 0, |inbox| {
                    inbox.child(kit::mention_badge(mentions, false, cx))
                })
                .child(
                    kit::icon_button("title-inbox-button", Lucide::Inbox, "Inbox")
                        .selected(inbox_open)
                        .on_click(cx.listener(|shell, _, _, cx| {
                            cx.stop_propagation();
                            shell.menu(MenuAction::OpenInbox, cx)
                        })),
                ),
        );

        bar = bar.child(name).child(search).child(actions);
        framed().child(bar).into_any_element()
    }
}

/// How much of the top and of the bottom of the screen the system draws its
/// own bars over, which the window lies under edge to edge.
#[cfg(target_os = "android")]
fn system_bars() -> (f32, f32) {
    gpui_mobile::android::jni::platform()
        .and_then(|platform| platform.primary_window())
        .map(|window| {
            let insets = window.safe_area_insets_logical();
            (insets.top, insets.bottom)
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "android"))]
fn system_bars() -> (f32, f32) {
    (0.0, 0.0)
}

/// The list of stored servers to pick from.
fn choose_server(servers: &[(String, String)], cx: &mut Context<Shell>) -> AnyElement {
    let theme = cx.theme().clone();
    let mut list = v_flex().w(px(420.)).max_w_full().gap_1().child(
        div()
            .pb_2()
            .text_xl()
            .text_center()
            .font_weight(FontWeight::SEMIBOLD)
            .child("Choose a server"),
    );
    for (index, (server, token)) in servers.iter().enumerate() {
        let (server_url, token) = (server.clone(), token.clone());
        list = list.child(
            v_flex()
                .id(("server", index))
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .cursor_pointer()
                .hover(|style| style.bg(theme.list_hover))
                .child(div().font_weight(FontWeight::MEDIUM).child(pretty_server(server)))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(server.clone()),
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.restore_session(server_url.clone(), token.clone(), window, cx)
                })),
        );
    }
    list = list.child(
        Button::new("add-server")
            .icon(Lucide::Plus)
            .label("Add Server…")
            .mt_2()
            .on_click(cx.listener(|shell, _, window, cx| shell.show_login(window, cx))),
    );
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .child(list)
        .into_any_element()
}

/// Remote screens and cameras, floating over the panes rather than in them:
/// a share is something you glance at while carrying on reading.
fn videos(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let views = ui.video_views.borrow();
    if views.is_empty() {
        return None;
    }
    let mut row = h_flex()
        .absolute()
        .left_3()
        .bottom_3()
        .gap_2()
        .items_end();
    for (index, (_, view)) in views.iter().enumerate() {
        let (width, height) = view.size();
        let expanded = view.expanded.get();
        let frame = div()
            .id(("video", index))
            .w(px(width))
            .h(px(height))
            .max_w_full()
            .rounded_md()
            .overflow_hidden()
            .bg(gpui_kit::black())
            .shadow_lg()
            .cursor_pointer()
            .when_some(view.image(), |frame, picture| {
                frame.child(img(picture).size_full().object_fit(ObjectFit::Contain))
            })
            .on_click(ui.click(move |ui, cx| {
                if let Some((_, view)) = ui.video_views.borrow().get(index) {
                    view.expanded.set(!view.expanded.get());
                }
                cx.refresh_windows();
            }));
        row = row.child(kit::with_tooltip(
            ("video-tip", index),
            frame,
            format!(
                "{} — click to {}",
                view.title,
                if expanded { "shrink" } else { "expand" }
            ),
        ));
    }
    let _ = cx;
    Some(row.into_any_element())
}

/// How wide a column is: a fraction of the window, within bounds.
fn column_width(window: f32, fraction: f32, min: f32, max: f32) -> f32 {
    (window * fraction).clamp(min, max)
}

/// How wide a side column is: what the person dragged it to if they did, its
/// share of the window otherwise. A dragged width may go past the share's own
/// bounds, but never so far that the conversation is squeezed out — `most` is
/// what is left for the column in this window, and wins over `least`.
fn dragged_width(dragged: Option<f32>, share: f32, least: f32, most: f32) -> f32 {
    dragged.map_or(share, |width| width.max(least)).min(most)
}

/// The thinnest a side column can be dragged, and the most of the window it
/// may take.
const DRAGGED_MIN: f32 = 180.0;
const DRAGGED_SHARE: f32 = 0.45;

/// What follows the pointer while a divider is dragged: nothing. The column
/// moving is the feedback.
struct DividerGhost;

impl Render for DividerGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// The strip that is dragged to resize a side column. It lies over the border
/// between two panes, a few pixels to either side, and draws nothing until
/// the pointer is on it. A double click gives the column back its share.
fn divider(ui: &Rc<Ui>, which: Divider, offset: f32, cx: &App) -> AnyElement {
    const GRIP: f32 = 7.0;
    let line = cx.theme().ring;
    let strip = div()
        .id(match which {
            Divider::Sidebar => "divider-sidebar",
            Divider::Panel => "divider-panel",
        })
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(GRIP))
        .flex()
        .justify_center()
        .cursor_col_resize()
        .occlude()
        .group("divider")
        .child(
            div()
                .w(px(2.))
                .h_full()
                .group_hover("divider", move |mark| mark.bg(line)),
        )
        .on_drag(which, |_, _, _, cx| cx.new(|_| DividerGhost))
        .on_click({
            let reset = ui.click(move |ui, cx| ui.widths.set(which, None, cx));
            move |click, window, cx| {
                if click.click_count() == 2 {
                    reset(click, window, cx);
                }
            }
        });
    match which {
        Divider::Sidebar => strip.left(px(offset - GRIP / 2.0)),
        Divider::Panel => strip.right(px(offset - GRIP / 2.0)),
    }
    .into_any_element()
}

/// The three panes, laid out for however wide the window is right now.
fn session(ui: &Rc<Ui>, window: &mut Window, cx: &mut App) -> AnyElement {
    ui.learn_unknown_mentions();
    let width = f32::from(window.viewport_size().width);
    ui.scale.set(window.scale_factor());
    let collapsed = width < SIDEBAR_PAGE_BELOW;
    let narrow = width < STATIC_PANEL_MIN_WIDTH;
    ui.split.collapsed.set(collapsed);
    ui.narrow.set(narrow);

    // A thread is a place you read alongside the conversation, so it earns a
    // static column when there is room. The inbox is a stack you glance at
    // and dismiss, so it always overlays — pushing the conversation aside for
    // it would be a heavier gesture than the content deserves.
    let overlays = narrow
        || matches!(
            ui.right.mode(cx),
            PanelMode::Inbox | PanelMode::Search(_)
        );
    let panel = ui
        .overlay
        .shown()
        .then(|| super::rhs::render(ui, cx))
        .flatten();
    let dock = super::call_dock::render(ui, cx);

    // On a phone the panel is a page: a strip of conversation left showing
    // beside it is too narrow to read and too easy to tap by accident.
    ui.widths.save_when_settled(cx);
    let most = (width * DRAGGED_SHARE).max(DRAGGED_MIN);
    let sidebar_width = dragged_width(
        ui.widths.get(Divider::Sidebar),
        column_width(width, 0.24, 220.0, 360.0),
        DRAGGED_MIN,
        most,
    );
    let panel_width = if collapsed {
        width
    } else {
        dragged_width(
            ui.widths.get(Divider::Panel),
            column_width(width, 0.30, 320.0, 460.0),
            DRAGGED_MIN,
            most,
        )
    };
    let mut panes = h_flex().relative().size_full().min_h_0().on_drag_move({
        let ui = ui.clone();
        move |moved: &DragMoveEvent<Divider>, _, cx| {
            let which = *moved.drag(cx);
            let pointer = moved.event.position.x;
            let dragged = match which {
                Divider::Sidebar => pointer - moved.bounds.left(),
                Divider::Panel => moved.bounds.right() - pointer,
            };
            ui.widths.set(which, Some(f32::from(dragged).round()), cx);
        }
    });

    if collapsed {
        // The conversation, and the channel list as a drawer that slides in
        // over it from the left. The dock would come and go with the drawer,
        // so here it belongs under the conversation.
        let drawer = (width * 0.86).min(360.0);
        let open = ui.split.advance(window);
        panes = panes.child(
            div()
                .size_full()
                .child(super::chat::render(ui, true, dock, cx)),
        );
        if open > 0.0 {
            panes = panes
                .child(
                    div()
                        .id("drawer-scrim")
                        .absolute()
                        .inset_0()
                        .bg(gpui_kit::black().opacity(0.45 * open))
                        .occlude()
                        .on_click(ui.click(|ui, cx| ui.split.set_show_content(true, cx))),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px((open - 1.0) * drawer))
                        .w(px(drawer))
                        .shadow_lg()
                        .occlude()
                        .child(super::sidebar::render(ui, None, cx)),
                );
        }
        // A phone has no edge to click: the list is pulled out by a swipe
        // to the right and pushed back by one to the left. Anything laid
        // over the conversation keeps its own swipes.
        if cfg!(target_os = "android") && !ui.overlay.shown() && !window.has_active_dialog(cx) {
            let ui = ui.clone();
            panes = panes.child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let delta = event.delta.pixel_delta(px(1.));
                            let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                            if ui.split.swiped(dx, dy, event.touch_phase, drawer) {
                                cx.stop_propagation();
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
    } else {
        panes = panes
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .w(px(sidebar_width))
                    .child(super::sidebar::render(ui, dock, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(super::chat::render(ui, false, None, cx)),
            );
    }

    if let Some(panel) = panel {
        panes = panes.child(if overlays || collapsed {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(panel_width))
                .shadow_lg()
                .occlude()
                .child(panel)
                .into_any_element()
        } else {
            div()
                .flex_none()
                .h_full()
                .w(px(panel_width))
                .child(panel)
                .into_any_element()
        });
        if !collapsed {
            panes = panes.child(divider(ui, Divider::Panel, panel_width, cx));
        }
    }
    // After the panes, so that it is above both of the two it sits between.
    if !collapsed {
        panes = panes.child(divider(ui, Divider::Sidebar, sidebar_width, cx));
    }

    panes
        .when_some(videos(ui, cx), |panes, videos| panes.child(videos))
        .when_some(super::lightbox::render(ui, cx), |panes, lightbox| {
            panes.child(lightbox)
        })
        .into_any_element()
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let notice = self.notice.borrow_mut().take();
        if let Some(notice) = notice {
            window.push_notification(
                gpui_kit::component::notification::Notification::new().message(notice),
                cx,
            );
        }

        let title_bar = self.title_bar(window, cx);
        let (bars_top, bars_bottom) = system_bars();
        let body = match &self.stage {
            Stage::Loading => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large())
                .into_any_element(),
            Stage::Login(view) => view.clone().into_any_element(),
            Stage::ChooseServer(servers) => choose_server(&servers.clone(), cx),
            Stage::Session(ui) => session(&ui.clone(), window, cx),
        };

        v_flex()
            .id("matterfast")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|shell, _: &QuickSwitch, _, cx| {
                shell.menu(MenuAction::QuickSwitch, cx)
            }))
            .on_action(cx.listener(|shell, _: &FocusSearch, _, cx| {
                shell.menu(MenuAction::FocusSearch, cx)
            }))
            .on_action(cx.listener(|shell, _: &OpenInbox, _, cx| {
                shell.menu(MenuAction::OpenInbox, cx)
            }))
            .on_action(cx.listener(|shell, _: &NewChannel, _, cx| {
                shell.menu(MenuAction::NewChannel, cx)
            }))
            .on_action(cx.listener(|shell, _: &OpenSettings, _, cx| {
                shell.menu(MenuAction::Settings, cx)
            }))
            .on_action(cx.listener(|shell, _: &NextUnread, _, cx| {
                shell.menu(MenuAction::NextUnread, cx)
            }))
            .on_action(cx.listener(|shell, _: &PreviousUnread, _, cx| {
                shell.menu(MenuAction::PreviousUnread, cx)
            }))
            .on_action(cx.listener(|shell, _: &ClosePanel, _, cx| {
                if !shell.close_front(cx) {
                    cx.propagate();
                }
            }))
            // The composer is where the focus nearly always is, and it has
            // its own idea of what Escape means. A picture over the window,
            // or an open panel, outranks that; a dialog or a completion list
            // does not, and gets the key as usual.
            .capture_action(cx.listener(|shell, _: &Escape, window, cx| {
                let busy = window.has_active_dialog(cx)
                    || shell.session().is_some_and(|ui| ui.chat.completing());
                if !busy && shell.close_front(cx) {
                    cx.stop_propagation();
                }
            }))
            // Under the status bar, in the title bar's colour so that the two
            // read as one.
            .child(div().flex_none().h(px(bars_top)).w_full().bg(theme.sidebar))
            .child(title_bar)
            .child(div().flex_1().min_h_0().w_full().child(body))
            .pb(px(bars_bottom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_two_positive_numbers() {
        assert_eq!(parse_size("400x800"), Some((400.0, 800.0)));
        assert_eq!(parse_size(" 1320 x 840 "), Some((1320.0, 840.0)));
        assert_eq!(parse_size("400"), None);
        assert_eq!(parse_size("wide x tall"), None);
        assert_eq!(parse_size("0x800"), None);
    }

    #[test]
    fn a_server_is_named_the_way_people_say_it() {
        assert_eq!(pretty_server("https://mm.example.com/"), "mm.example.com");
        assert_eq!(pretty_server("http://localhost:8065"), "localhost:8065");
    }

    #[test]
    fn a_dragged_column_keeps_its_width_and_leaves_room_for_the_conversation() {
        // Untouched, it is its share of the window.
        assert_eq!(dragged_width(None, 316.8, 180.0, 594.0), 316.8);
        // Dragged, it is what it was dragged to, past the share's own bounds.
        assert_eq!(dragged_width(Some(500.0), 316.8, 180.0, 594.0), 500.0);
        assert_eq!(dragged_width(Some(40.0), 316.8, 180.0, 594.0), 180.0);
        // A width chosen in a wide window does not swallow a narrow one.
        assert_eq!(dragged_width(Some(500.0), 220.0, 180.0, 360.0), 360.0);
    }

    #[test]
    fn a_column_is_a_fraction_of_the_window_within_bounds() {
        // The sidebar: a quarter of the window, never thinner than a channel
        // name nor wider than one needs.
        assert_eq!(column_width(1320.0, 0.24, 220.0, 360.0), 316.8);
        assert_eq!(column_width(700.0, 0.24, 220.0, 360.0), 220.0);
        assert_eq!(column_width(2560.0, 0.24, 220.0, 360.0), 360.0);
    }
}
