use serde_json::json;

use crate::app::{apply_reaction, App};
use crate::clock::now;
use crate::constants::{DEV, DM_LENA, GENERAL, LENA, ME, MIKK, SARA};

pub(crate) fn seed(app: &App) {
    // A scrollback long enough to page through. Seven seeded posts fit on one
    // screen, so pagination and scroll-anchor work cannot be exercised without
    // it. `MM_HISTORY=400` is what the profiling runs use.
    let history: usize = std::env::var("MM_HISTORY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for i in 0..history {
        let author = [SARA, MIKK, LENA, ME][i % 4];
        app.add_post(DEV, author, &format!("Backlog message {}", i + 1), "");
    }

    let root = app.add_post(
        DEV,
        SARA,
        "Reminder: MediaMap.sender_id is the *receiver's* session id. Parse the track id instead.",
        "",
    );
    let root_id = root["id"].as_str().unwrap().to_string();
    app.add_post(
        DEV,
        MIKK,
        "That explains the mis-attributed screen shares.",
        &root_id,
    );
    app.add_post(
        DEV,
        ME,
        "Adding a test for the screen-audio case.",
        &root_id,
    );

    app.add_post(
        DEV,
        LENA,
        "Release notes draft is up — shout if the Calls section overstates things.",
        "",
    );
    app.add_post(
        DEV,
        MIKK,
        "@anton staging rejects binary websocket frames, hold the release.",
        "",
    );
    app.add_post(GENERAL, SARA, "Standup moved to 10:15 tomorrow.", "");
    app.add_post(GENERAL, LENA, "@backend who is @on-call this week?", "");
    app.add_post(DM_LENA, LENA, "Did the reconnect fix land?", "");
    app.add_post(
        DM_LENA,
        LENA,
        "Merged :shipit: and it is live :party_blob: :tada: — typed as `:shipit:`, and a:zz9:c is not one.",
        "",
    );

    // Mentions of every kind: you, somebody else, somebody the client has
    // never been told about, and a whole channel.
    app.add_post(
        DEV,
        MIKK,
        "@anton and @lena pair on this; @olga has the context. @channel heads up.",
        "",
    );

    // What an integration posts: a card with things to press on it. Pressing
    // one comes back through `post_action`, which answers the way an
    // integration does — by editing the post.
    let card = app.add_post(DEV, SARA, "", "");
    let attachments = json!([{
        "fallback": "Deploy 4.2.1 to production?",
        "color": "#2eb886",
        "author_name": "Deploy bot",
        "title": "Deploy 4.2.1 to production?",
        "text": "Staging has been green for **2 hours**.",
        "fields": [
            { "title": "Branch", "value": "release/4.2", "short": true },
            { "title": "Commits", "value": 14, "short": true },
        ],
        "actions": [
            { "id": "approve", "type": "button", "name": "Approve", "style": "success" },
            { "id": "reject", "type": "button", "name": "Reject", "style": "danger" },
            { "id": "later", "name": "Not now", "style": "#7c3aed" },
            {
                "id": "window", "type": "select", "name": "Pick a window",
                "options": [
                    { "text": "Tonight", "value": "tonight" },
                    { "text": "Tomorrow morning", "value": "tomorrow" },
                ],
            },
            { "id": "owner", "type": "select", "name": "Assign to", "data_source": "users" },
        ],
        "footer": "ci.example.com",
    }]);
    let mut db = app.db.lock().unwrap();
    if let Some(stored) = db.posts.iter_mut().find(|p| p.v["id"] == card["id"]) {
        stored.v["props"] = json!({ "attachments": attachments, "from_webhook": "true" });
    }
    drop(db);

    // A couple of reactions on the root, so the emoji rendering has something
    // to show without anyone clicking.
    for (user_id, emoji) in [(LENA, "eyes"), (MIKK, "eyes"), (SARA, "tada")] {
        let reaction = json!({
            "user_id": user_id, "post_id": root_id, "emoji_name": emoji,
            "create_at": now(), "update_at": now(), "delete_at": 0, "channel_id": DEV,
        });
        apply_reaction(app, &root_id, &reaction, true);
    }
}
