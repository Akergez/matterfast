//! The sign-in view.
//!
//! Authenticates with `POST /users/login` and keeps the session token that
//! comes back in the `Token` **response header** — the body is just the user.

use adw::prelude::*;
use mattermost_api::{Client, Error, User};

use crate::runtime;

pub struct LoginResult {
    pub client: Client,
    pub me: User,
}

/// Builds the sign-in page. `on_success` runs on the GTK thread once we hold a
/// session.
pub fn build(on_success: impl Fn(LoginResult) + Clone + 'static) -> gtk::Widget {
    let server = adw::EntryRow::builder().title("Server URL").build();
    server.set_text("https://");
    let login_id = adw::EntryRow::builder().title("Email or username").build();
    let password = adw::PasswordEntryRow::builder().title("Password").build();
    let mfa = adw::EntryRow::builder()
        .title("MFA code (if enabled)")
        .build();
    let ldap = adw::SwitchRow::builder()
        .title("Domain (LDAP) sign-in")
        .subtitle("For servers using the LDAP auth plugin")
        .build();

    let group = adw::PreferencesGroup::builder()
        .title("Sign in to Mattermost")
        .description("Your credentials are sent only to the server you name here.")
        .build();
    group.add(&server);
    group.add(&login_id);
    group.add(&password);
    group.add(&mfa);
    group.add(&ldap);

    let button = gtk::Button::builder()
        .label("Sign in")
        .halign(gtk::Align::Center)
        .margin_top(18)
        .build();
    button.add_css_class("suggested-action");
    button.add_css_class("pill");

    let error = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .margin_top(12)
        .visible(false)
        .build();
    error.add_css_class("error");

    // adw::Spinner needs libadwaita 1.6; GTK's own works everywhere.
    let spinner = gtk::Spinner::builder()
        .visible(false)
        .height_request(24)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .valign(gtk::Align::Center)
        .build();
    content.append(&group);
    content.append(&button);
    content.append(&spinner);
    content.append(&error);

    let clamp = adw::Clamp::builder()
        .maximum_size(460)
        .margin_start(18)
        .margin_end(18)
        .child(&content)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build(),
    ));

    // The click handler is shared by the button and the Enter key on each row.
    let submit = {
        let server = server.clone();
        let login_id = login_id.clone();
        let password = password.clone();
        let mfa = mfa.clone();
        let ldap = ldap.clone();
        let error = error.clone();
        let spinner = spinner.clone();
        let button = button.clone();
        let on_success = on_success.clone();

        move || {
            let url = server.text().trim().to_string();
            let user = login_id.text().trim().to_string();
            let pass = password.text().to_string();
            let code = mfa.text().trim().to_string();
            let use_ldap = ldap.is_active();

            if url.is_empty() || user.is_empty() || pass.is_empty() {
                show_error(&error, "Server, username and password are all required.");
                return;
            }

            let client = match Client::new(&url) {
                Ok(c) => c,
                Err(e) => {
                    show_error(&error, &format!("That server URL is not valid: {e}"));
                    return;
                }
            };

            error.set_visible(false);
            spinner.set_visible(true);
            spinner.start();
            button.set_sensitive(false);

            let mfa_code = if code.is_empty() { None } else { Some(code) };
            let request_client = client.clone();
            let error = error.clone();
            let spinner = spinner.clone();
            let button = button.clone();
            let on_success = on_success.clone();

            runtime::spawn(
                async move {
                    let me = if use_ldap {
                        request_client.login_ldapauth(&user, &pass).await
                    } else {
                        request_client
                            .login(&user, &pass, mfa_code.as_deref())
                            .await
                    };
                    me.map(|me| (request_client, me))
                },
                move |result| {
                    spinner.stop();
                    spinner.set_visible(false);
                    button.set_sensitive(true);
                    match result {
                        Ok((client, me)) => on_success(LoginResult { client, me }),
                        Err(e) => show_error(&error, &describe(&e)),
                    }
                },
            );
        }
    };

    button.connect_clicked({
        let submit = submit.clone();
        move |_| submit()
    });
    for row in [&server, &login_id, &mfa] {
        row.connect_entry_activated({
            let submit = submit.clone();
            move |_| submit()
        });
    }
    password.connect_entry_activated({
        let submit = submit.clone();
        move |_| submit()
    });

    toolbar.upcast()
}

fn show_error(label: &gtk::Label, message: &str) {
    label.set_text(message);
    label.set_visible(true);
}

/// Turns an API error into something worth showing a person.
fn describe(error: &Error) -> String {
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
