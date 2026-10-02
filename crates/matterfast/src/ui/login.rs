//! The sign-in view.
//!
//! Authenticates with `POST /users/login` and keeps the session token that
//! comes back in the `Token` **response header** — the body is just the user.
//! Single sign-on takes a different route entirely; see [`super::sso`].

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, img, px, App, Context, Entity, FontWeight, Image, ImageFormat, Subscription, Window,
};
use mattermost_api::{Client, Error, User};

use crate::runtime;

pub struct LoginResult {
    pub client: Client,
    pub me: User,
}

/// The application's own icon, for the top of the form.
fn app_icon() -> std::sync::Arc<Image> {
    static ICON: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        std::sync::Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../../../../data/icons/hicolor/scalable/apps/io.gitlab.akergez.Matterfast.svg")
                .to_vec(),
        ))
    })
    .clone()
}

/// What the form is validated into before anything is sent.
#[derive(Debug, PartialEq, Eq)]
struct Credentials {
    url: String,
    user: String,
    password: String,
    mfa: Option<String>,
}

/// Reads the four fields. An error is something to say under the form.
fn credentials(url: &str, user: &str, password: &str, mfa: &str) -> Result<Credentials, String> {
    let url = url.trim();
    let user = user.trim();
    if url.is_empty() || url == "https://" || user.is_empty() || password.is_empty() {
        return Err("Server, username and password are all required.".to_string());
    }
    let mfa = mfa.trim();
    Ok(Credentials {
        url: url.to_string(),
        user: user.to_string(),
        password: password.to_string(),
        mfa: (!mfa.is_empty()).then(|| mfa.to_string()),
    })
}

pub struct LoginView {
    server: Entity<InputState>,
    login_id: Entity<InputState>,
    password: Entity<InputState>,
    mfa: Entity<InputState>,
    ldap: bool,
    busy: bool,
    /// Nothing more happens until the browser sends us back, and the button
    /// has to say so — otherwise the window looks stuck.
    waiting_for_browser: bool,
    /// The server's single sign-on providers, once it has been asked and
    /// offered more than one; a button each.
    providers: Vec<super::sso::Provider>,
    error: Option<String>,
    on_success: Rc<dyn Fn(LoginResult, &mut App)>,
    _subscriptions: Vec<Subscription>,
}

impl LoginView {
    /// Builds the sign-in page. `on_success` runs once we hold a session.
    pub fn new(
        on_success: impl Fn(LoginResult, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let server = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("https://")
                .placeholder("https://mattermost.example.com")
        });
        let login_id = cx.new(|cx| InputState::new(window, cx).placeholder("Email or username"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });
        let mfa = cx.new(|cx| InputState::new(window, cx).placeholder("MFA code (if enabled)"));

        // Enter in any field is the same as the button.
        let subscriptions = [&server, &login_id, &password, &mfa]
            .into_iter()
            .map(|field| {
                cx.subscribe(field, |view: &mut LoginView, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        view.submit(cx);
                    }
                })
            })
            .collect();

        LoginView {
            server,
            login_id,
            password,
            mfa,
            ldap: false,
            busy: false,
            waiting_for_browser: false,
            providers: Vec::new(),
            error: None,
            on_success: Rc::new(on_success),
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let credentials = match credentials(
            &self.server.read(cx).value(),
            &self.login_id.read(cx).value(),
            &self.password.read(cx).value(),
            &self.mfa.read(cx).value(),
        ) {
            Ok(credentials) => credentials,
            Err(message) => {
                self.error = Some(message);
                cx.notify();
                return;
            }
        };
        let client = match Client::new(&credentials.url) {
            Ok(client) => client,
            Err(e) => {
                self.error = Some(format!("That server URL is not valid: {e}"));
                cx.notify();
                return;
            }
        };

        self.error = None;
        self.busy = true;
        cx.notify();

        let use_ldap = self.ldap;
        let view = cx.entity();
        let on_success = self.on_success.clone();
        runtime::spawn(
            async move {
                let me = if use_ldap {
                    client
                        .login_ldapauth(&credentials.user, &credentials.password)
                        .await
                } else {
                    client
                        .login(
                            &credentials.user,
                            &credentials.password,
                            credentials.mfa.as_deref(),
                        )
                        .await
                };
                me.map(|me| (client, me))
            },
            move |result, cx| {
                view.update(cx, |view, cx| {
                    view.busy = false;
                    if let Err(e) = &result {
                        view.error = Some(describe(e));
                    }
                    cx.notify();
                });
                if let Ok((client, me)) = result {
                    on_success(LoginResult { client, me }, cx);
                }
            },
        );
    }

    /// SSO needs nothing but the server: the browser collects the credentials
    /// and the answer comes back through the `mattermost-dev://` scheme.
    fn single_sign_on(&mut self, cx: &mut Context<Self>) {
        let url = self.server.read(cx).value().trim().to_string();
        if url.is_empty() || url == "https://" {
            self.error = Some("Fill in the server URL first.".to_string());
            cx.notify();
            return;
        }
        let client = match Client::new(&url) {
            Ok(client) => client,
            Err(e) => {
                self.error = Some(format!("That server URL is not valid: {e}"));
                cx.notify();
                return;
            }
        };

        // Which provider is the server's to say, so ask before opening
        // anything: with one there is nothing to choose, with several the
        // form grows a button for each.
        self.error = None;
        self.busy = true;
        self.providers.clear();
        cx.notify();
        let view = cx.entity();
        runtime::spawn(
            {
                let client = client.clone();
                async move { client.client_config().await }
            },
            move |result, cx| {
                view.update(cx, |view, cx| {
                    view.busy = false;
                    match result.as_ref().map(super::sso::providers) {
                        Ok(found) => match found.as_slice() {
                            [] => {
                                view.error = Some(
                                    "This server does not offer single sign-on. \
                                     Sign in with a username and password."
                                        .to_string(),
                                )
                            }
                            [only] => view.open_browser(client, only.clone(), cx),
                            _ => view.providers = found,
                        },
                        Err(e) => view.error = Some(describe(e)),
                    }
                    cx.notify();
                });
            },
        );
    }

    /// Sends the browser to one provider and waits for it to send us back.
    fn open_browser(
        &mut self,
        client: Client,
        provider: super::sso::Provider,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        let view = cx.entity().downgrade();
        let on_success = self.on_success.clone();
        let started = super::sso::start(
            client,
            &provider,
            move |result, cx| on_success(result, cx),
            move |message, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.waiting_for_browser = false;
                    view.error = Some(message.to_string());
                    cx.notify();
                });
            },
            cx,
        );
        match started {
            Ok(()) => self.waiting_for_browser = true,
            Err(message) => self.error = Some(message),
        }
        cx.notify();
    }
}

impl Render for LoginView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let field = |label: &'static str, input: &Entity<InputState>| {
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                .child(Input::new(input))
        };

        let form = v_flex()
            .w(px(420.))
            .max_w_full()
            .gap_3()
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(img(app_icon()).size(px(96.)))
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Sign in to Mattermost"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(theme.muted_foreground)
                            .child(
                                "Your credentials are sent only to the server you name here.",
                            ),
                    ),
            )
            .child(field("Server URL", &self.server))
            .child(field("Email or username", &self.login_id))
            .child(field("Password", &self.password))
            .child(field("MFA code (if enabled)", &self.mfa))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .child("Domain (LDAP) sign-in")
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("For servers using the LDAP auth plugin"),
                            ),
                    )
                    .child(Switch::new("ldap").checked(self.ldap).on_click(cx.listener(
                        |view, on: &bool, _, cx| {
                            view.ldap = *on;
                            cx.notify();
                        },
                    ))),
            )
            .child(
                Button::new("sign-in")
                    .label("Sign in")
                    .primary()
                    .w_full()
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, _, _, cx| view.submit(cx))),
            )
            .child(
                Button::new("sso")
                    .label(if self.waiting_for_browser {
                        "Waiting for the browser…"
                    } else {
                        "Single sign-on"
                    })
                    .w_full()
                    .disabled(self.waiting_for_browser || self.busy)
                    .on_click(cx.listener(|view, _, _, cx| view.single_sign_on(cx))),
            )
            .children(self.providers.iter().enumerate().map(|(index, provider)| {
                let provider = provider.clone();
                Button::new(("provider", index))
                    .label(format!("Sign in with {}", provider.name))
                    .w_full()
                    .disabled(self.waiting_for_browser)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let url = view.server.read(cx).value().trim().to_string();
                        if let Ok(client) = Client::new(&url) {
                            view.open_browser(client, provider.clone(), cx);
                        }
                    }))
            }))
            .when(self.busy, |form| {
                form.child(h_flex().justify_center().child(Spinner::new()))
            })
            .when_some(self.error.clone(), |form, error| {
                form.child(
                    div()
                        .text_center()
                        .text_color(theme.danger)
                        .child(error),
                )
            });

        div()
            .id("login")
            .size_full()
            .overflow_y_scroll()
            .bg(theme.background)
            .child(
                div()
                    .min_h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .child(form),
            )
    }
}

/// Turns an API error into something worth showing a person.
pub fn describe(error: &Error) -> String {
    match error {
        Error::Api(app) if app.is_mfa_required() => {
            "This account needs a multi-factor code — fill in the MFA field.".to_string()
        }
        Error::Api(app) if app.status_code == 401 => {
            "That username or password was not accepted.".to_string()
        }
        Error::Api(app) => app.message.clone(),
        // A rejected certificate also surfaces as a connect error, and saying
        // "check the URL" for it sends people looking in the wrong place.
        e if e.is_cert_failure() => {
            "The server's TLS certificate was not trusted. Set MM_INSECURE_TLS=1 \
             to connect anyway, or install the issuing CA."
                .to_string()
        }
        Error::Http(e) if e.is_connect() => {
            "Could not reach that server. Check the URL and your connection.".to_string()
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_sent_until_the_form_is_filled_in() {
        let required = Err("Server, username and password are all required.".to_string());
        assert_eq!(credentials("", "anna", "secret", ""), required);
        // The field starts out holding the scheme, which is not a server.
        assert_eq!(credentials("https://", "anna", "secret", ""), required);
        assert_eq!(credentials("https://mm.test", "  ", "secret", ""), required);
        assert_eq!(credentials("https://mm.test", "anna", "", ""), required);
    }

    #[test]
    fn the_fields_are_trimmed_but_the_password_is_not() {
        assert_eq!(
            credentials(" https://mm.test ", " anna ", " secret ", " 123456 "),
            Ok(Credentials {
                url: "https://mm.test".into(),
                user: "anna".into(),
                // A space is a legal character in a password.
                password: " secret ".into(),
                mfa: Some("123456".into()),
            })
        );
        // No code typed is no code sent, not an empty one.
        assert_eq!(
            credentials("https://mm.test", "anna", "secret", "  ")
                .unwrap()
                .mfa,
            None
        );
    }
}
