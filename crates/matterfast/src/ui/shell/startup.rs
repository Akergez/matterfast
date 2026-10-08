use gpui_kit::{Context, Window};

use super::matterfast::current;
use super::shell_view::Shell;
use super::stage::Stage;
use super::with_shell::with_shell;
use crate::runtime;
use crate::ui::Action;

impl Shell {
    /// Decides what this window is for. Runs once, right after it opens.
    pub(super) fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    ui.dispatch(Action::LoadChannel(id), cx);
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
}
