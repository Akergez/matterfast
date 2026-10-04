use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{AppContext, Context, Window};
use mattermost_api::models::{ClientConfig, User};

use super::matterfast::Matterfast;
use super::shell_view::Shell;
use super::signed_out::signed_out;
use super::stage::Stage;
use super::with_shell::with_shell;
use crate::runtime;
use crate::state::AppState;
use crate::ui::login::{LoginResult, LoginView};
use crate::ui::Ui;

impl Shell {
    pub(super) fn show_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session_subscriptions.clear();
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
    pub(super) fn adopt(
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
    pub(super) fn show_session(&mut self, ui: Rc<Ui>, window: &mut Window, cx: &mut Context<Self>) {
        let (composer, composer_events) = crate::ui::chat::build_composer(&ui, window, cx);
        let (reply, reply_events) = crate::ui::rhs::build_composer(&ui, window, cx);
        let (search, search_events) = crate::ui::search::build(&ui, window, cx);
        ui.chat.attach(composer);
        ui.right.attach(reply);
        ui.search_box.attach(search);
        self.session_subscriptions = vec![composer_events, reply_events, search_events];
        self.stage = Stage::Session(ui.clone());
        cx.notify();

        // The rest wants the window, and this is the window in the middle of
        // being set up.
        let handle = window.window_handle();
        ui.later(cx, move |ui, cx| ui.attach(handle, cx));
    }

    /// Replaces the login view with the main UI and kicks off the startup
    /// sequence.
    pub(super) fn start_session(
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
        ui.later(cx, |ui, cx| crate::ui::bootstrap(ui.clone(), None, cx));
    }

    /// Tries a stored token before showing the sign-in form. The form is
    /// shown if the token is gone or stale — a session Mattermost has since
    /// revoked is the ordinary case here, not an error worth a dialog.
    pub(super) fn restore_session(
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
            crate::ui::bootstrap(ui.clone(), Some(Box::new(signed_out)), cx)
        });
    }
}
