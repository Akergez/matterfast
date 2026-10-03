//! Single sign-on through the browser, using Mattermost's desktop-token flow.
//!
//! The server will not redirect an OAuth callback to a loopback address —
//! `ValidateWebAuthRedirectUrl` insists the redirect matches the site's own
//! scheme and host — so there is no port to listen on. What it will do is hand
//! the result to a registered URL scheme:
//!
//! 1. we invent a `client_token` and send the browser to
//!    `/oauth/<service>/login?desktop_token=<client_token>`,
//! 2. the identity provider authenticates the person,
//! 3. Mattermost mints a `server_token` and bounces the browser to
//!    `/login/desktop`, whose page deep-links to `mattermost-dev://…`,
//! 4. that launches us again; the new process hands the URI to the running
//!    one (see [`crate::ipc`]), and we trade the `server_token` for a session.
//!
//! The `dev-` prefix on the client token is load-bearing: it is what makes the
//! server pick the `mattermost-dev` scheme over `mattermost`, which belongs to
//! the official desktop app.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::ClientConfig;
use mattermost_api::Client;

use super::login::LoginResult;
use crate::runtime;

/// One way a server lets people in through the browser.
#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    /// What the button says, after "Sign in with".
    pub name: String,
    /// Where the browser starts, relative to the site.
    path: &'static str,
}

/// The providers a server has switched on, read from the part of its config
/// it publishes before anybody is signed in.
///
/// Asking matters: a server answers a provider it has not enabled with an
/// error page, so guessing one — it used to be GitLab, always — locks out
/// everybody whose company signs in some other way.
pub fn providers(config: &ClientConfig) -> Vec<Provider> {
    // A server may rename the generic ones after its identity provider.
    let named = |key: &str, fallback: &str| {
        config
            .get(key)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or(fallback)
            .to_string()
    };
    let known = [
        ("EnableSignUpWithOpenId", named("OpenIdButtonText", "OpenID Connect"), "/oauth/openid/login"),
        ("EnableSaml", named("SamlLoginButtonText", "SAML"), "/login/sso/saml"),
        ("EnableSignUpWithGitLab", named("GitLabButtonText", "GitLab"), "/oauth/gitlab/login"),
        ("EnableSignUpWithGoogle", "Google".to_string(), "/oauth/google/login"),
        ("EnableSignUpWithOffice365", "Entra ID".to_string(), "/oauth/office365/login"),
    ];
    known
        .into_iter()
        .filter(|(key, _, _)| config.bool(key))
        .map(|(_, name, path)| Provider { name, path })
        .collect()
}

struct Pending {
    client_token: String,
    client: Client,
    on_success: Rc<dyn Fn(LoginResult, &mut App)>,
    on_error: Rc<dyn Fn(&str, &mut App)>,
}

thread_local! {
    /// At most one sign-in can be in flight, and it only ever touches the main
    /// thread, so a thread-local outlives the login view without an `Rc` maze.
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

/// Opens the browser at the provider's login page. Returns the error to show
/// if the browser could not be launched at all.
pub fn start(
    client: Client,
    provider: &Provider,
    on_success: impl Fn(LoginResult, &mut App) + 'static,
    on_error: impl Fn(&str, &mut App) + 'static,
    cx: &mut App,
) -> Result<(), String> {
    let client_token = new_client_token();
    let url = format!(
        "{}{}?desktop_token={client_token}",
        client.site_url().trim_end_matches('/'),
        provider.path,
    );

    // The path only: the token in the query is half of the sign-in.
    tracing::info!(path = provider.path, "opening the browser to sign in");
    cx.open_url(&url);

    PENDING.with(|p| {
        *p.borrow_mut() = Some(Pending {
            client_token,
            client,
            on_success: Rc::new(on_success),
            on_error: Rc::new(on_error),
        })
    });
    Ok(())
}

/// Handles a `mattermost-dev://…` callback. Anything that is not one, or does
/// not match the sign-in we started, is dropped: this URI arrives from outside
/// the process and a stray one must not be able to log anybody in.
pub fn deliver(uri: &str, cx: &mut App) {
    let Some((client_token, server_token)) = parse_callback(uri) else {
        tracing::warn!("ignoring an SSO callback that carried no tokens");
        return;
    };

    let Some(pending) = PENDING.with(|p| p.borrow_mut().take()) else {
        tracing::warn!("an SSO callback arrived with no sign-in in flight");
        return;
    };
    if pending.client_token != client_token {
        tracing::warn!("an SSO callback carried a client token we did not issue");
        (pending.on_error)("That sign-in did not match the one this window started.", cx);
        return;
    }

    tracing::info!("the browser came back; trading its token for a session");
    let client = pending.client.clone();
    let on_success = pending.on_success.clone();
    let on_error = pending.on_error.clone();
    runtime::spawn(
        async move {
            client
                .login_with_desktop_token(&server_token)
                .await
                .map(|me| (client, me))
        },
        move |result, cx| match result {
            Ok((client, me)) => on_success(LoginResult { client, me }, cx),
            // A 401 here is a spent or expired desktop token, not a bad
            // password — `describe` would otherwise say the wrong thing.
            Err(mattermost_api::Error::Api(app)) if app.status_code == 401 => {
                on_error("That sign-in did not complete. Try again.", cx)
            }
            Err(e) => {
                tracing::warn!(error = %e, "the sign-in token was not accepted");
                on_error(&super::login::describe(&e), cx)
            }
        },
    );
}

/// Pulls the two tokens out of `mattermost-dev://host/login/desktop?…`.
fn parse_callback(uri: &str) -> Option<(String, String)> {
    let uri = uri.strip_prefix("mattermost-dev://")?;
    let query = uri.split_once('?')?.1;
    let mut client_token = None;
    let mut server_token = None;
    // Both tokens are alphanumeric by construction — the server mints them with
    // `NewRandomString`, we mint ours as hex — so there is nothing to decode.
    for (key, value) in query.split('&').filter_map(|p| p.split_once('=')) {
        match key {
            "client_token" => client_token = Some(value.to_string()),
            "server_token" => server_token = Some(value.to_string()),
            _ => {}
        }
    }
    Some((client_token?, server_token?))
}

/// 64 characters, `dev-` and then hex — the shape the web app uses, and the
/// prefix the server keys the `mattermost-dev` scheme off.
fn new_client_token() -> String {
    use rand::Rng;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut rng = rand::rng();
    let mut token = String::from("dev-");
    while token.len() < 64 {
        token.push(HEX[rng.random_range(0..HEX.len())] as char);
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> ClientConfig {
        ClientConfig(
            pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        )
    }

    #[test]
    fn only_the_providers_a_server_enabled_are_offered() {
        let found = providers(&config(&[
            ("EnableSignUpWithGitLab", "false"),
            ("EnableSignUpWithOpenId", "true"),
            ("OpenIdButtonText", "Keycloak"),
            ("EnableSaml", "true"),
        ]));
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Keycloak", "SAML"]);
        assert_eq!(found[0].path, "/oauth/openid/login");
        assert_eq!(found[1].path, "/login/sso/saml");
    }

    #[test]
    fn a_server_with_passwords_only_offers_none() {
        assert!(providers(&config(&[("EnableSignInWithEmail", "true")])).is_empty());
        // A blank button text is not a name.
        let found = providers(&config(&[
            ("EnableSignUpWithOpenId", "true"),
            ("OpenIdButtonText", " "),
        ]));
        assert_eq!(found[0].name, "OpenID Connect");
    }

    #[test]
    fn callback_parsing_and_token_shape() {
        let uri = "mattermost-dev://mm.example.com/login/desktop\
                   ?client_token=dev-abc&server_token=xyz&isDesktopDev=true";
        assert_eq!(
            parse_callback(uri),
            Some(("dev-abc".to_string(), "xyz".to_string()))
        );
        // A callback missing either half must not be treated as a sign-in.
        assert_eq!(
            parse_callback("mattermost-dev://h/login/desktop?client_token=a"),
            None
        );
        assert_eq!(
            parse_callback("https://mm.example.com/login/desktop?a=b"),
            None
        );

        let token = new_client_token();
        assert_eq!(token.len(), 64);
        assert!(token.starts_with("dev-"));
        assert!(token[4..].chars().all(|c| c.is_ascii_hexdigit()));
        // Not a constant: two sign-ins must not share a token.
        assert_ne!(token, new_client_token());
    }
}
