use std::sync::Arc;

use crate::app::App;
use crate::constants::{DEV, DM_LENA, GENERAL, LENA, ME, MIKK, SARA};
use crate::model::{filler_count, filler_id};

/// Posts every few seconds so live updates are visible with one client open.
pub(crate) async fn colleague(app: Arc<App>) {
    let lines = [
        (LENA, DEV, "Rebased onto the reconnect branch.", ""),
        (
            MIKK,
            DEV,
            "@anton can you look at the ICE buffering patch?",
            "",
        ),
        (
            SARA,
            GENERAL,
            "Docs for the data channel are in the wiki now.",
            "",
        ),
        (LENA, DM_LENA, "It did — thanks.", ""),
    ];
    let mut i = 0usize;
    loop {
        // Slow enough that a human — or a screenshot test — can click on
        // something before the list moves under them.
        let interval = std::env::var("MM_BOT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(12);
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        let (user_id, channel_id, message, root) = lines[i % lines.len()];
        // With generated channels present, spread the noise across them: the
        // sidebar's redraw cost is per row, and it only shows when the badges
        // move around a long list rather than in the same three places.
        let channel_id = match filler_count() {
            0 => channel_id.to_string(),
            n => filler_id(i % n),
        };
        i += 1;
        let post = app.add_post(&channel_id, user_id, message, root);
        let mentions = if message.contains("@anton") {
            vec![ME]
        } else {
            vec![]
        };
        app.post_channel_event(&post, mentions);
    }
}
