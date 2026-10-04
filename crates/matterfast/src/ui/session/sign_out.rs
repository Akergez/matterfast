use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::{dialogs, shell};

impl Ui {
    /// Signs out of this server, forgetting its token and its cached
    /// messages, and leaves any other server signed in.
    pub(crate) fn sign_out(self: &Rc<Self>, cx: &mut App) {
        let ui = self.clone();
        dialogs::confirm(
            self,
            cx,
            "Sign out?",
            "This device will forget the session. Anything unsent is lost.",
            "Sign Out",
            true,
            move |cx| {
                let client = ui.state.borrow().client.clone();
                let server = client.site_url().to_string();
                crate::cache::clear();
                let stored = server.clone();
                runtime::spawn(
                    async move {
                        if let Err(e) = crate::store::Store::clear(&stored).await {
                            tracing::warn!(error = %e, "could not delete the local messages");
                        }
                    },
                    |_, _| {},
                );
                runtime::spawn(
                    async move {
                        // Revoke it server-side too, so a copy that leaked with
                        // the file is useless rather than merely forgotten.
                        let _ = client.logout().await;
                        crate::session::forget_async(&server).await;
                    },
                    |_, _| {},
                );
                shell::signed_out(cx);
            },
        );
    }
}
