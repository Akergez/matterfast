//! A fake Mattermost server, just real enough to exercise the client.
//!
//! It is not a mock in the unit-test sense — it holds actual state, assigns
//! real ids, and pushes the same websocket events the real server does,
//! including the double-encoded payloads (`posted.post` as a JSON *string*) and
//! the inconsistent key casing. Getting those wrong here would defeat the
//! purpose: the point is to catch the client mis-parsing what the server
//! actually sends.
//!
//! ```sh
//! cargo run -p matterfast-testserver           # listens on 127.0.0.1:8065
//! ```
//!
//! Log in with any username and password. A background "colleague" posts every
//! few seconds so live updates are visible without a second client.
//!
//! Under `/zed` it is also a fake of the one other server the client talks
//! to: the Zed editor's extension registry, from which themes are installed.
//! `MATTERFAST_THEMES_API=http://127.0.0.1:8065/zed` points the client at it,
//! so installing a theme can be tested with no network.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Request;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::broadcast;

const ME: &str = "uuuuuuuuuuuuuuuuuuuuuuuume";
const LENA: &str = "uuuuuuuuuuuuuuuuuuuuuulena";
const MIKK: &str = "uuuuuuuuuuuuuuuuuuuuuumikk";
/// Somebody the client is never told about: in no channel and on no list, so
/// the only way to reach her is the server's own search.
const OLGA: &str = "uuuuuuuuuuuuuuuuuuuuuuolga";
const SARA: &str = "uuuuuuuuuuuuuuuuuuuuuusara";
const TEAM: &str = "tttttttttttttttttttttttcor";
const GENERAL: &str = "cccccccccccccccccccccgener";
const DEV: &str = "ccccccccccccccccccccccdeve";
const DM_LENA: &str = "cccccccccccccccccccccdmlen";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Mattermost ids are exactly 26 lowercase alphanumerics, and the client
/// validates that, so pad rather than truncate.
fn id(prefix: &str, n: i64) -> String {
    let mut s = String::from(prefix);
    let tail = format!("{n}");
    while s.len() + tail.len() < 26 {
        s.push('0');
    }
    s.push_str(&tail);
    s.truncate(26);
    s
}

struct Post {
    v: Value,
}

#[derive(Default)]
struct Db {
    posts: Vec<Post>,
    next_id: i64,
}

struct App {
    db: Mutex<Db>,
    events: broadcast::Sender<Value>,
    seq: AtomicI64,
    conn: AtomicI64,
}

impl App {
    /// Broadcasts an event with the server's own monotonic sequence, which the
    /// client validates strictly.
    fn emit(&self, event: &str, data: Value, broadcast_: Value) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let _ = self.events.send(json!({
            "event": event,
            "data": data,
            "broadcast": broadcast_,
            "seq": seq,
        }));
    }

    fn add_post_with_pending(
        &self,
        channel_id: &str,
        user_id: &str,
        message: &str,
        root_id: &str,
        pending_post_id: &str,
    ) -> Value {
        let mut post = self.add_post(channel_id, user_id, message, root_id);
        // The real server echoes this back so clients can retire their
        // optimistic copy; not doing so makes every sent message appear twice.
        post["pending_post_id"] = json!(pending_post_id);
        let post_id = post["id"].clone();
        let mut db = self.db.lock().unwrap();
        if let Some(stored) = db.posts.iter_mut().find(|p| p.v["id"] == post_id) {
            stored.v["pending_post_id"] = json!(pending_post_id);
        }
        post
    }

    fn add_post(&self, channel_id: &str, user_id: &str, message: &str, root_id: &str) -> Value {
        let mut db = self.db.lock().unwrap();
        db.next_id += 1;
        let post_id = id("p", db.next_id);
        // Seeding happens inside one millisecond, and a real server would not
        // hand out identical timestamps for a whole conversation — nudge each
        // post forward so ordering is deterministic.
        let at = now() + db.next_id;
        let post = json!({
            "id": post_id,
            "create_at": at,
            "update_at": at,
            "edit_at": 0,
            "delete_at": 0,
            "is_pinned": false,
            "user_id": user_id,
            "channel_id": channel_id,
            "root_id": root_id,
            "original_id": "",
            "message": message,
            "type": "",
            "props": {},
            "hashtags": "",
            "file_ids": [],
            "pending_post_id": "",
            "reply_count": 0,
            "last_reply_at": 0,
            "participants": null,
            "metadata": {},
        });
        db.posts.push(Post { v: post.clone() });

        // Keep the root's reply_count honest — the client renders it.
        if !root_id.is_empty() {
            let replies = db
                .posts
                .iter()
                .filter(|p| p.v["root_id"] == root_id)
                .count() as i64;
            if let Some(root) = db.posts.iter_mut().find(|p| p.v["id"] == root_id) {
                root.v["reply_count"] = json!(replies);
                root.v["last_reply_at"] = json!(at);
            }
        }
        post
    }

    fn post_channel_event(&self, post: &Value, mentions: Vec<&str>) {
        let channel_id = post["channel_id"].as_str().unwrap_or_default().to_string();
        let user_id = post["user_id"].as_str().unwrap_or_default();
        self.emit(
            "posted",
            json!({
                // The real server sends the post as a JSON *string*.
                "post": serde_json::to_string(post).unwrap(),
                "channel_type": if channel_id == DM_LENA { "D" } else { "O" },
                "channel_display_name": channel_name(&channel_id),
                "channel_name": channel_id,
                "sender_name": username(user_id),
                "team_id": TEAM,
                "set_online": true,
                "mentions": serde_json::to_string(&mentions).unwrap(),
            }),
            json!({ "channel_id": channel_id, "team_id": TEAM }),
        );
    }
}

fn username(user_id: &str) -> &'static str {
    match user_id {
        LENA => "lena",
        MIKK => "mikk",
        SARA => "sara",
        _ => "anton",
    }
}

fn channel_name(channel_id: &str) -> String {
    match channel_id {
        DEV => "Development".to_string(),
        DM_LENA => String::new(),
        _ => match filler_index(channel_id) {
            Some(n) => format!("Channel {n:02}"),
            None => "General".to_string(),
        },
    }
}

/// Extra open channels beyond the three hand-written ones. Three rows rebuild
/// too fast to time, so any work on how the sidebar redraws itself needs a
/// realistic list to show up at all — `MM_CHANNELS=100` is what the sidebar
/// profiling runs use.
fn filler_count() -> usize {
    std::env::var("MM_CHANNELS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn filler_id(n: usize) -> String {
    id("cx", n as i64)
}

/// The inverse of `filler_id`, so the per-channel handlers can answer for a
/// generated channel without keeping a table of them.
fn filler_index(channel_id: &str) -> Option<usize> {
    channel_id.strip_prefix("cx")?.parse().ok()
}

fn user(user_id: &str) -> Value {
    let (username, first, last, position) = match user_id {
        LENA => ("lena", "Lena", "Petrova", "Media engineer"),
        MIKK => ("mikk", "Mikk", "Tamm", "Release manager"),
        SARA => ("sara", "Sara", "Okafor", "Protocol archaeologist"),
        OLGA => ("olga", "Olga", "Belova", "On another team"),
        _ => ("anton", "Anton", "Ivanov", "Desktop client"),
    };
    json!({
        "id": user_id,
        "create_at": 1_700_000_000_000i64,
        "update_at": 1_700_000_000_000i64,
        "delete_at": 0,
        "username": username,
        "first_name": first,
        "last_name": last,
        "nickname": "",
        "email": format!("{username}@example.com"),
        "position": position,
        "roles": "system_user",
        "locale": "en",
        "last_picture_update": 1_700_000_000_000i64,
        "timezone": {"useAutomaticTimezone": "true", "automaticTimezone": "Europe/Tallinn", "manualTimezone": ""},
        "props": if user_id == SARA {
            json!({"customStatus": r#"{"emoji":"coffee","text":"Reading rtcd","duration":"today"}"#})
        } else { json!({}) },
        "notify_props": {},
    })
}

fn channel(channel_id: &str) -> Value {
    if let Some(n) = filler_index(channel_id) {
        return json!({
            "id": channel_id,
            "create_at": 1_700_000_000_000i64,
            "update_at": 1_700_000_000_000i64,
            "delete_at": 0,
            "team_id": TEAM,
            "type": "O",
            "display_name": channel_name(channel_id),
            "name": format!("channel-{n}"),
            "header": "",
            "purpose": "",
            "last_post_at": now(),
            "total_msg_count": 6,
            "total_msg_count_root": 6,
            "creator_id": SARA,
        });
    }
    let (name, display, kind, total) = match channel_id {
        DEV => ("development", "Development", "O", 12),
        DM_LENA => (
            // A DM's name is the two ids joined by a double underscore.
            Box::leak(format!("{ME}__{LENA}").into_boxed_str()) as &str,
            "",
            "D",
            3,
        ),
        _ => ("general", "General", "O", 6),
    };
    json!({
        "id": channel_id,
        "create_at": 1_700_000_000_000i64,
        "update_at": 1_700_000_000_000i64,
        "delete_at": 0,
        "team_id": if kind == "D" { "" } else { TEAM },
        "type": kind,
        "display_name": display,
        "name": name,
        "header": if channel_id == DEV { "Client work · protocol notes pinned" } else { "" },
        "purpose": "",
        "last_post_at": now(),
        "total_msg_count": total,
        "total_msg_count_root": total,
        "creator_id": SARA,
    })
}

fn membership(channel_id: &str, unread: i64, mentions: i64) -> Value {
    let total = channel(channel_id)["total_msg_count"].as_i64().unwrap_or(0);
    json!({
        "channel_id": channel_id,
        "user_id": ME,
        "roles": "channel_user",
        "last_viewed_at": now(),
        "msg_count": total - unread,
        "msg_count_root": total - unread,
        "mention_count": mentions,
        "mention_count_root": mentions,
        "urgent_mention_count": 0,
        "notify_props": {},
        "last_update_at": now(),
        "scheme_admin": false,
    })
}

// ────────────────────────────────────────────────────────────── handlers

async fn login(State(app): State<Arc<App>>) -> Response {
    let _ = &app;
    let mut headers = HeaderMap::new();
    // The token comes back in a *header*, not the body.
    headers.insert("Token", "test-session-token".parse().unwrap());
    (headers, Json(user(ME))).into_response()
}

async fn client_config() -> Json<Value> {
    Json(json!({
        "Version": "11.11.0",
        "SiteName": "Test Mattermost",
        "TeammateNameDisplay": "full_name",
        // Collapsed reply threads on, so the client exercises the CRT path.
        "CollapsedThreads": "default_on",
        "EnableCustomEmoji": "true",
        "PostPriority": "true",
    }))
}

/// The one extension the fake registry has. Its theme is in Zed's format,
/// trailing comma and comment included, because published themes have them.
const ZED_THEME: &str = r##"{
  // A theme nobody else has, so a test cannot pass by finding a built-in one.
  "name": "Testserver",
  "author": "matterfast-testserver",
  "themes": [
    {
      "name": "Testserver Dusk",
      "appearance": "dark",
      "style": {
        "background": "#1b1d2aff",
        "editor.background": "#14151fff",
        "panel.background": "#1b1d2aff",
        "elevated_surface.background": "#22243355",
        "element.background": "#262a3cff",
        "element.hover": "#30354bff",
        "ghost_element.hover": "#30354b80",
        "text": "#d6d9eaff",
        "text.muted": "#8f94b3ff",
        "text.accent": "#f2a65aff",
        "border": "#30354bff",
        "syntax": { "comment": { "color": "#8f94b3ff", "font_style": "italic" } },
      }
    }
  ]
}"##;

async fn zed_extensions() -> Json<Value> {
    Json(json!({ "data": [{
        "id": "testserver-dusk",
        "name": "Testserver Dusk",
        "version": "1.0.0",
        "description": "A theme served by the fake registry",
        "authors": ["Test Server <test@example.invalid>"],
        "repository": "https://example.invalid/testserver-dusk",
        "schema_version": 1,
        "wasm_api_version": null,
        "provides": ["themes"],
        "published_at": "2026-01-01T00:00:00Z",
        "download_count": 1234
    }] }))
}

/// An extension the way the registry packages one: `extension.toml` and a
/// `themes` directory, gzipped.
async fn zed_extension_download(Path(id): Path<String>) -> Response {
    if id != "testserver-dusk" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    for (path, text) in [
        ("./extension.toml", "id = \"testserver-dusk\"\n"),
        ("./themes/testserver-dusk.json", ZED_THEME),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, path, text.as_bytes()).unwrap();
    }
    let bytes = archive.into_inner().unwrap().finish().unwrap();
    ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
}

async fn empty_object() -> Json<Value> {
    Json(json!({}))
}

async fn empty_array() -> Json<Value> {
    Json(json!([]))
}

async fn me() -> Json<Value> {
    Json(user(ME))
}

async fn my_teams() -> Json<Value> {
    Json(json!([{
        "id": TEAM,
        "create_at": 1_700_000_000_000i64,
        "update_at": 1_700_000_000_000i64,
        "delete_at": 0,
        "display_name": "Core Platform",
        "name": "core",
        "description": "",
        "type": "O",
        "allow_open_invite": false,
    }]))
}

async fn my_team_members() -> Json<Value> {
    Json(json!([{
        "team_id": TEAM, "user_id": ME, "roles": "team_user",
        "delete_at": 0, "scheme_user": true, "scheme_admin": false,
    }]))
}

async fn my_channels() -> Json<Value> {
    let mut out = vec![channel(GENERAL), channel(DEV), channel(DM_LENA)];
    out.extend((0..filler_count()).map(|n| channel(&filler_id(n))));
    Json(Value::Array(out))
}

async fn my_channel_members() -> Json<Value> {
    let mut out = vec![
        membership(GENERAL, 0, 0),
        membership(DEV, 4, 1),
        membership(DM_LENA, 1, 0),
    ];
    // Not all alike: a few carry mentions and a few are already read, so the
    // sidebar has badges, dots and plain rows to redraw rather than one shape.
    out.extend((0..filler_count()).map(|n| {
        membership(
            &filler_id(n),
            (n % 4) as i64,
            if n % 7 == 0 { 1 + (n % 3) as i64 } else { 0 },
        )
    }));
    Json(Value::Array(out))
}

async fn categories() -> Json<Value> {
    let mut channels = vec![GENERAL.to_string(), DEV.to_string()];
    channels.extend((0..filler_count()).map(filler_id));
    Json(json!({
        "categories": [
            {"id": "channels_cat", "user_id": ME, "team_id": TEAM, "sort_order": 10,
             "sorting": "alpha", "type": "channels", "display_name": "Channels",
             "muted": false, "collapsed": false, "channel_ids": channels},
            {"id": "dm_cat", "user_id": ME, "team_id": TEAM, "sort_order": 20,
             "sorting": "recent", "type": "direct_messages", "display_name": "Direct Messages",
             "muted": false, "collapsed": false, "channel_ids": [DM_LENA]}
        ],
        "order": ["channels_cat", "dm_cat"]
    }))
}

fn post_list(app: &App, filter: impl Fn(&Value) -> bool) -> Value {
    let db = app.db.lock().unwrap();
    let mut matched: Vec<&Value> = db
        .posts
        .iter()
        .map(|p| &p.v)
        .filter(|p| filter(p))
        .collect();
    matched.sort_by_key(|p| -p["create_at"].as_i64().unwrap_or(0));

    let order: Vec<String> = matched
        .iter()
        .map(|p| p["id"].as_str().unwrap_or_default().to_string())
        .collect();
    let mut posts = serde_json::Map::new();
    for p in matched {
        posts.insert(p["id"].as_str().unwrap_or_default().to_string(), p.clone());
    }
    json!({
        "order": order,
        "posts": posts,
        // Empty both ways means "this page is the whole channel".
        "next_post_id": "",
        "prev_post_id": "",
        "first_inaccessible_post_time": 0,
    })
}

async fn channel_posts(
    State(app): State<Arc<App>>,
    Path(channel_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let since: i64 = q.get("since").and_then(|s| s.parse().ok()).unwrap_or(0);
    let crt = q.get("collapsedThreads").map(String::as_str) == Some("true");
    let before = q.get("before").map(String::as_str);
    let page = q.get("page").and_then(|s| s.parse().ok()).unwrap_or(0usize);
    let per_page = q
        .get("per_page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60usize)
        .clamp(1, 200);

    let db = app.db.lock().unwrap();
    let mut matched: Vec<Value> = db
        .posts
        .iter()
        .map(|p| &p.v)
        .filter(|p| {
            p["channel_id"] == channel_id.as_str()
                && p["update_at"].as_i64().unwrap_or(0) > since
                // Under CRT, replies live in threads and never in the channel feed.
                && (!crt || p["root_id"].as_str().unwrap_or_default().is_empty())
        })
        .cloned()
        .collect();
    matched.sort_by_key(|p| -p["create_at"].as_i64().unwrap_or(0));

    let start = before
        .and_then(|id| matched.iter().position(|post| post["id"] == id))
        .map_or_else(|| page.saturating_mul(per_page), |position| position + 1)
        .min(matched.len());
    let end = start.saturating_add(per_page).min(matched.len());
    let page = &matched[start..end];
    let order: Vec<String> = page
        .iter()
        .filter_map(|post| post["id"].as_str().map(str::to_string))
        .collect();
    let posts: serde_json::Map<String, Value> = page
        .iter()
        .filter_map(|post| Some((post["id"].as_str()?.to_string(), post.clone())))
        .collect();
    Json(json!({
        "order": order,
        "posts": posts,
        "next_post_id": if start > 0 { "newer" } else { "" },
        "prev_post_id": if end < matched.len() { "older" } else { "" },
        "first_inaccessible_post_time": 0,
    }))
}

async fn unread_posts(
    State(app): State<Arc<App>>,
    Path(channel_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let crt = q.get("collapsedThreads").map(String::as_str) == Some("true");
    Json(post_list(&app, |p| {
        p["channel_id"] == channel_id.as_str()
            && (!crt || p["root_id"].as_str().unwrap_or_default().is_empty())
    }))
}

async fn post_thread(State(app): State<Arc<App>>, Path(post_id): Path<String>) -> Json<Value> {
    Json(post_list(&app, |p| {
        p["id"] == post_id.as_str() || p["root_id"] == post_id.as_str()
    }))
}

async fn create_post(State(app): State<Arc<App>>, Json(body): Json<Value>) -> Json<Value> {
    let channel_id = body["channel_id"].as_str().unwrap_or_default();
    let message = body["message"].as_str().unwrap_or_default();
    let root_id = body["root_id"].as_str().unwrap_or_default();
    let pending = body["pending_post_id"].as_str().unwrap_or_default();
    let post = app.add_post_with_pending(channel_id, ME, message, root_id, pending);
    app.post_channel_event(&post, vec![]);
    Json(post)
}

async fn users_by_ids(Json(ids): Json<Vec<String>>) -> Json<Value> {
    Json(Value::Array(ids.iter().map(|i| user(i)).collect()))
}

async fn users_query(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let _ = q;
    Json(json!([user(ME), user(LENA), user(MIKK), user(SARA)]))
}

/// `POST /users/search` — the whole directory, which is more than any client
/// has loaded.
async fn users_search(Json(body): Json<Value>) -> Json<Value> {
    Json(Value::Array(directory(body["term"].as_str().unwrap_or_default())))
}

/// Everybody whose handle or name begins with `term`, Olga included: she is
/// on the server and in nothing the client has opened.
fn directory(term: &str) -> Vec<Value> {
    let term = term.to_lowercase();
    [ME, LENA, MIKK, SARA, OLGA]
        .into_iter()
        .map(user)
        .filter(|u| {
            ["username", "first_name", "last_name"]
                .iter()
                .any(|key| u[key].as_str().unwrap_or_default().to_lowercase().starts_with(&term))
        })
        .collect()
}

/// `GET /users/autocomplete?name=…` — what the `@` list and the search box's
/// `from:` list ask. With no channel named there is no `out_of_channel`.
async fn users_autocomplete(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let name = q.get("name").map(String::as_str).unwrap_or_default();
    Json(json!({ "users": directory(name) }))
}

/// `POST /users/usernames` — how a client resolves a mention of somebody it
/// has not met.
async fn users_by_usernames(Json(names): Json<Vec<String>>) -> Json<Value> {
    let found = [ME, LENA, MIKK, SARA, OLGA]
        .into_iter()
        .map(user)
        .filter(|u| names.iter().any(|name| u["username"] == name.as_str()))
        .collect();
    Json(Value::Array(found))
}

/// `GET /groups` — the groups that can be mentioned. One of them has no
/// mention name, as a directory group nobody has set up does.
async fn groups(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let first = q.get("page").is_none_or(|page| page == "0");
    Json(if first {
        json!([
            {"id": "group-backend", "name": "backend", "display_name": "Backend team",
             "source": "custom", "allow_reference": true, "member_count": 3, "delete_at": 0},
            {"id": "group-oncall", "name": "on-call", "display_name": "On call",
             "source": "custom", "allow_reference": true, "member_count": 1, "delete_at": 0},
            {"id": "group-ldap", "name": null, "display_name": "Directory only",
             "source": "ldap", "allow_reference": true, "member_count": 2, "delete_at": 0},
        ])
    } else {
        json!([])
    })
}

/// `GET /groups/{id}/members` — who a group mention reaches.
async fn group_members(Path(id): Path<String>) -> Json<Value> {
    let members: Vec<Value> = match id.as_str() {
        "group-backend" => vec![user(LENA), user(MIKK), user(OLGA)],
        "group-oncall" => vec![user(SARA)],
        _ => Vec::new(),
    };
    Json(json!({ "total_member_count": members.len(), "members": members }))
}

async fn statuses(Json(ids): Json<Vec<String>>) -> Json<Value> {
    Json(Value::Array(
        ids.iter()
            .map(|i| {
                let status = match i.as_str() {
                    LENA => "online",
                    MIKK => "away",
                    SARA => "dnd",
                    _ => "online",
                };
                json!({"user_id": i, "status": status, "manual": false,
                       "last_activity_at": now(), "dnd_end_time": 0})
            })
            .collect(),
    ))
}

/// A flat colour per user, so "the right avatar for the right person" is
/// verifiable at a glance.
/// The server's own emoji: a still and an animation, so both ways of drawing
/// one are exercised.
const EMOJI: [&str; 2] = ["party_blob", "shipit"];

fn emoji(name: &str) -> Value {
    json!({"id": format!("emoji-{name}"), "name": name, "creator_id": ME,
           "create_at": 1, "update_at": 1, "delete_at": 0})
}

async fn emoji_list(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    // One page holds them all; any later page is empty.
    let first = q.get("page").is_none_or(|page| page == "0");
    Json(Value::Array(if first {
        EMOJI.iter().map(|name| emoji(name)).collect()
    } else {
        Vec::new()
    }))
}

async fn emoji_search(Json(body): Json<Value>) -> Json<Value> {
    let term = body["term"].as_str().unwrap_or_default().to_lowercase();
    Json(Value::Array(
        EMOJI
            .iter()
            .filter(|name| name.contains(&term))
            .map(|name| emoji(name))
            .collect(),
    ))
}

async fn emoji_by_name(Path(name): Path<String>) -> Response {
    if EMOJI.contains(&name.as_str()) {
        Json(emoji(&name)).into_response()
    } else {
        axum::http::StatusCode::NOT_FOUND.into_response()
    }
}

/// `shipit` is a green disc; `party_blob` is a disc that changes colour, as
/// an animated GIF, so it is plain on screen whether the frames are playing.
async fn emoji_image(Path(id): Path<String>) -> Response {
    let disc = |rgb: [u8; 3]| {
        image::RgbaImage::from_fn(64, 64, |x, y| {
            let (dx, dy) = (x as f32 - 31.5, y as f32 - 31.5);
            if dx * dx + dy * dy < 30.0 * 30.0 {
                image::Rgba([rgb[0], rgb[1], rgb[2], 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        })
    };
    if id == "emoji-party_blob" {
        let mut gif = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .unwrap();
            for rgb in [[0xE8, 0x3A, 0x5A], [0xF2, 0xB1, 0x1D], [0x3B, 0x74, 0xD8]] {
                let delay = image::Delay::from_numer_denom_ms(300, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(disc(rgb), 0, 0, delay))
                    .unwrap();
            }
        }
        return ([(header::CONTENT_TYPE, "image/gif")], gif).into_response();
    }
    let mut png = std::io::Cursor::new(Vec::new());
    disc([0x2E, 0x9E, 0x6B])
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    ([(header::CONTENT_TYPE, "image/png")], png.into_inner()).into_response()
}

async fn user_image(Path(user_id): Path<String>) -> Response {
    let rgb = match user_id.as_str() {
        LENA => [0xE8, 0x6A, 0x33],
        MIKK => [0x2E, 0x9E, 0x6B],
        SARA => [0x3B, 0x74, 0xD8],
        _ => [0xC9, 0x9A, 0x1E],
    };
    let mut img = image::RgbImage::new(128, 128);
    for (x, y, px) in img.enumerate_pixels_mut() {
        // A diagonal band makes it obvious this is a real image, not a colour
        // block the client painted itself.
        let band = ((x + y) / 16) % 2 == 0;
        let scale = if band { 1.0 } else { 0.82 };
        *px = image::Rgb([
            (rgb[0] as f32 * scale) as u8,
            (rgb[1] as f32 * scale) as u8,
            (rgb[2] as f32 * scale) as u8,
        ]);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();

    ([(header::CONTENT_TYPE, "image/png")], png.into_inner()).into_response()
}

async fn add_reaction(State(app): State<Arc<App>>, Json(body): Json<Value>) -> Json<Value> {
    let post_id = body["post_id"].as_str().unwrap_or_default().to_string();
    let emoji = body["emoji_name"].as_str().unwrap_or_default().to_string();
    let reaction = json!({
        "user_id": ME, "post_id": post_id, "emoji_name": emoji,
        "create_at": now(), "update_at": now(), "delete_at": 0,
        "channel_id": "",
    });
    apply_reaction(&app, &post_id, &reaction, true);
    app.emit(
        "reaction_added",
        // Again: a JSON string, not an object.
        json!({ "reaction": serde_json::to_string(&reaction).unwrap() }),
        json!({ "channel_id": "" }),
    );
    Json(reaction)
}

/// A button or a menu on a card. The real server forwards this to the
/// integration and applies its answer; this plays both parts, replacing the
/// buttons with what was decided and announcing the edit.
async fn post_action(
    State(app): State<Arc<App>>,
    Path((post_id, action_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let selected = body["selected_option"].as_str().unwrap_or_default();
    let outcome = match (action_id.as_str(), selected) {
        (action, "") => format!("anton pressed `{action}`."),
        (action, selected) => format!("anton set `{action}` to `{selected}`."),
    };
    let edited = {
        let mut db = app.db.lock().unwrap();
        let Some(post) = db.posts.iter_mut().find(|p| p.v["id"] == post_id) else {
            return Json(json!({ "status": "OK" }));
        };
        let card = &mut post.v["props"]["attachments"][0];
        card["actions"] = Value::Null;
        card["text"] = json!(outcome);
        post.v["update_at"] = json!(now());
        post.v.clone()
    };
    app.emit(
        "post_edited",
        json!({ "post": serde_json::to_string(&edited).unwrap() }),
        json!({ "channel_id": edited["channel_id"] }),
    );
    Json(json!({ "status": "OK", "trigger_id": "" }))
}

async fn remove_reaction(
    State(app): State<Arc<App>>,
    Path((user_id, post_id, emoji)): Path<(String, String, String)>,
) -> StatusCode {
    let reaction = json!({
        "user_id": user_id, "post_id": post_id, "emoji_name": emoji,
        "create_at": 0, "update_at": 0, "delete_at": now(), "channel_id": "",
    });
    apply_reaction(&app, &post_id, &reaction, false);
    app.emit(
        "reaction_removed",
        json!({ "reaction": serde_json::to_string(&reaction).unwrap() }),
        json!({ "channel_id": "" }),
    );
    StatusCode::OK
}

fn apply_reaction(app: &App, post_id: &str, reaction: &Value, add: bool) {
    let mut db = app.db.lock().unwrap();
    let Some(post) = db.posts.iter_mut().find(|p| p.v["id"] == post_id) else {
        return;
    };
    let list = post.v["metadata"]["reactions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut list: Vec<Value> = list;
    if add {
        list.push(reaction.clone());
    } else {
        list.retain(|r| {
            !(r["user_id"] == reaction["user_id"] && r["emoji_name"] == reaction["emoji_name"])
        });
    }
    post.v["has_reactions"] = json!(!list.is_empty());
    post.v["metadata"]["reactions"] = json!(list);
}

async fn view_channel() -> Json<Value> {
    Json(json!({"status": "OK", "last_viewed_at_times": {}}))
}

async fn team_unreads() -> Json<Value> {
    Json(json!([{
        "team_id": TEAM, "msg_count": 5, "mention_count": 1,
        "mention_count_root": 1, "msg_count_root": 5,
        "thread_count": 1, "thread_mention_count": 1, "thread_urgent_mention_count": 0,
    }]))
}

/// `GET /users/{me}/teams/{team}/threads` — the CRT inbox.
async fn threads(State(app): State<Arc<App>>) -> Json<Value> {
    let db = app.db.lock().unwrap();
    let roots: Vec<Value> = db
        .posts
        .iter()
        .map(|p| &p.v)
        .filter(|p| p["reply_count"].as_i64().unwrap_or(0) > 0)
        .map(|p| {
            json!({
                "id": p["id"],
                "reply_count": p["reply_count"],
                "last_reply_at": p["last_reply_at"],
                "last_viewed_at": 0,
                "unread_replies": p["reply_count"],
                "unread_mentions": 0,
                "is_following": true,
                "post": p,
                "participants": [user(SARA), user(ME)],
            })
        })
        .collect();
    Json(json!({
        "threads": roots,
        "total": 0,
        "total_unread_threads": 0,
        "total_unread_mentions": 0,
    }))
}

/// A search as the real server reads one: words that must all be there (or
/// any of them, for an "or" search), and the modifiers that narrow where and
/// when. Enough of the grammar for a client to be tested against: `from:`,
/// `in:`, `before:`, `after:`, `on:` and `-word`.
#[derive(Debug, Default, PartialEq)]
struct SearchTerms {
    words: Vec<String>,
    excluded: Vec<String>,
    from: Vec<String>,
    channels: Vec<String>,
    /// Days since the Unix epoch, in the asker's time zone.
    before: Option<i64>,
    after: Option<i64>,
    on: Option<i64>,
}

fn parse_search(terms: &str) -> SearchTerms {
    let mut parsed = SearchTerms::default();
    for token in terms.to_lowercase().split_whitespace() {
        let token = token.trim_matches('"');
        if let Some(name) = token.strip_prefix("from:") {
            parsed.from.push(name.trim_start_matches('@').to_string());
        } else if let Some(name) = token.strip_prefix("in:") {
            parsed.channels.push(name.trim_start_matches('~').to_string());
        } else if let Some(day) = token.strip_prefix("before:") {
            parsed.before = day_number(day);
        } else if let Some(day) = token.strip_prefix("after:") {
            parsed.after = day_number(day);
        } else if let Some(day) = token.strip_prefix("on:") {
            parsed.on = day_number(day);
        } else if let Some(word) = token.strip_prefix('-').filter(|word| !word.is_empty()) {
            parsed.excluded.push(word.to_string());
        } else if !token.trim_start_matches('@').is_empty() {
            parsed.words.push(token.trim_start_matches('@').to_string());
        }
    }
    parsed
}

/// `2026-10-03` as days since the Unix epoch; nothing for what is not a date.
fn day_number(text: &str) -> Option<i64> {
    let mut parts = text.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from the civil calendar, with the year starting in March so that
    // the leap day is the last day of it.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

impl SearchTerms {
    /// Whether a post is a hit. `offset` is the asker's time zone, in
    /// seconds east of UTC: a day is theirs, not Greenwich's.
    fn matches(&self, post: &Value, any_word: bool, offset: i64) -> bool {
        let message = post["message"].as_str().unwrap_or_default().to_lowercase();
        let has = |word: &String| message.contains(word.as_str());
        let words = if any_word {
            self.words.is_empty() || self.words.iter().any(has)
        } else {
            self.words.iter().all(has)
        };
        if !words || self.excluded.iter().any(has) {
            return false;
        }
        let author = username(post["user_id"].as_str().unwrap_or_default());
        if !self.from.is_empty() && !self.from.iter().any(|name| name == author) {
            return false;
        }
        if !self.channels.is_empty() {
            let found = channel(post["channel_id"].as_str().unwrap_or_default());
            // A direct message is named by the other person, as `@handle`.
            let name = if found["type"] == "D" {
                "@lena".to_string()
            } else {
                found["name"].as_str().unwrap_or_default().to_string()
            };
            if !self.channels.iter().any(|wanted| *wanted == name) {
                return false;
            }
        }
        let day = (post["create_at"].as_i64().unwrap_or(0) / 1000 + offset).div_euclid(86_400);
        self.before.is_none_or(|before| day < before)
            && self.after.is_none_or(|after| day > after)
            && self.on.is_none_or(|on| day == on)
    }
}

async fn search_posts(State(app): State<Arc<App>>, Json(body): Json<Value>) -> Json<Value> {
    let terms = parse_search(body["terms"].as_str().unwrap_or_default());
    let any_word = body["is_or_search"].as_bool().unwrap_or(false);
    let offset = body["time_zone_offset"].as_i64().unwrap_or(0);
    let mut list = post_list(&app, |post| terms.matches(post, any_word, offset));

    // One page of it, newest first, as the real server cuts it.
    let per_page = body["per_page"].as_u64().unwrap_or(60).max(1) as usize;
    let page = body["page"].as_u64().unwrap_or(0) as usize;
    let order: Vec<Value> = list["order"]
        .as_array()
        .map(|order| order.iter().skip(page * per_page).take(per_page).cloned().collect())
        .unwrap_or_default();
    if let Some(posts) = list["posts"].as_object_mut() {
        posts.retain(|id, _| order.iter().any(|kept| kept == id));
    }
    // What a real server searching its own database was seen to say about
    // which words matched: every hit is named, and has `null` for its words —
    // a nil slice, as Go writes one. A client that expects a list there fails
    // on every search that finds something, which is how this was noticed.
    let matches: serde_json::Map<String, Value> = order
        .iter()
        .filter_map(|id| Some((id.as_str()?.to_string(), Value::Null)))
        .collect();
    list["order"] = order.into();
    list["matches"] = matches.into();
    Json(list)
}

// ────────────────────────────────────────────────────────────── websocket

async fn websocket(ws: WebSocketUpgrade, State(app): State<Arc<App>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, app))
}

async fn handle_socket(socket: WebSocket, app: Arc<App>) {
    use futures_util::{SinkExt, StreamExt};
    let (mut sink, mut source) = socket.split();

    let conn_id = id("w", app.conn.fetch_add(1, Ordering::SeqCst));
    let mut events = app.events.subscribe();

    // `hello` is always sequence 0 of a stream.
    app.seq.store(0, Ordering::SeqCst);
    let hello = json!({
        "event": "hello",
        "data": {"connection_id": conn_id, "server_version": "11.11.0.test"},
        "broadcast": {},
        "seq": app.seq.fetch_add(1, Ordering::SeqCst),
    });
    if sink
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            event = events.recv() => {
                let Ok(event) = event else { continue };
                if sink.send(Message::Text(event.to_string().into())).await.is_err() {
                    return;
                }
            }
            incoming = source.next() => {
                let Some(Ok(msg)) = incoming else { return };
                let Message::Text(text) = msg else { continue };
                let Ok(req) = serde_json::from_str::<Value>(&text) else { continue };
                // The client pings every 30s and closes the socket if the reply
                // does not arrive before the next tick.
                if req["action"] == "ping" {
                    let reply = json!({
                        "status": "OK",
                        "seq_reply": req["seq"],
                        "data": {"text": "pong", "version": "11.11.0"},
                    });
                    if sink.send(Message::Text(reply.to_string().into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

// ────────────────────────────────────────────────────────────── seed & bot

fn seed(app: &App) {
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

/// Posts every few seconds so live updates are visible with one client open.
async fn colleague(app: Arc<App>) {
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

#[tokio::main]
async fn main() {
    let (events, _) = broadcast::channel(256);
    let app = Arc::new(App {
        db: Mutex::new(Db::default()),
        events,
        seq: AtomicI64::new(0),
        conn: AtomicI64::new(1),
    });
    seed(&app);
    tokio::spawn(colleague(app.clone()));

    let api = Router::new()
        .route("/users/login", post(login))
        .route("/users/logout", post(empty_object))
        .route("/config/client", get(client_config))
        .route("/license/client", get(empty_object))
        .route("/users/me", get(me))
        .route("/users/me/preferences", get(empty_array))
        .route("/users/me/teams", get(my_teams))
        .route("/users/me/teams/members", get(my_team_members))
        .route("/users/me/teams/unread", get(team_unreads))
        .route("/users/me/teams/{team}/channels", get(my_channels))
        .route(
            "/users/me/teams/{team}/channels/members",
            get(my_channel_members),
        )
        .route(
            "/users/me/teams/{team}/channels/categories",
            get(categories),
        )
        .route("/users/me/teams/{team}/threads", get(threads))
        .route("/users/{user}/teams/{team}/threads", get(threads))
        .route("/users/ids", post(users_by_ids))
        .route("/users/search", post(users_search))
        .route("/users/autocomplete", get(users_autocomplete))
        .route("/users/usernames", post(users_by_usernames))
        .route("/users", get(users_query))
        .route("/users/status/ids", post(statuses))
        .route("/users/{user}/image", get(user_image))
        .route(
            "/users/me/channels/{channel}/posts/unread",
            get(unread_posts),
        )
        .route("/channels/{channel}/posts", get(channel_posts))
        .route("/channels/members/me/view", post(view_channel))
        .route("/posts", post(create_post))
        .route("/posts/{post}/thread", get(post_thread))
        .route("/posts/{post}/actions/{action}", post(post_action))
        .route("/reactions", post(add_reaction))
        .route(
            "/users/{user}/posts/{post}/reactions/{emoji}",
            delete(remove_reaction),
        )
        .route("/teams/{team}/posts/search", post(search_posts))
        .route("/emoji", get(emoji_list))
        .route("/groups", get(groups))
        .route("/groups/{id}/members", get(group_members))
        .route("/emoji/search", post(emoji_search))
        .route("/emoji/name/{name}", get(emoji_by_name))
        .route("/emoji/{id}/image", get(emoji_image))
        .route("/websocket", get(websocket));

    // Request logging: without it there is no way to tell "the client never
    // asked" from "the server answered wrongly", which is most of debugging.
    let zed = Router::new()
        .route("/extensions", get(zed_extensions))
        .route("/extensions/{id}/download", get(zed_extension_download));

    let router = Router::new()
        .nest("/api/v4", api)
        .with_state(app)
        .nest("/zed", zed)
        .layer(middleware::from_fn(|req: Request, next: Next| async move {
            let method = req.method().clone();
            let path = req.uri().to_string();
            let response = next.run(req).await;
            println!("{method} {path} -> {}", response.status().as_u16());
            response
        }));

    let address =
        std::env::var("MM_TESTSERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8065".to_string());
    let listener = tokio::net::TcpListener::bind(&address).await.unwrap();
    println!("fake Mattermost on http://{address} — any username and password will do");
    axum::serve(listener, router).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_its_number_of_days_since_1970() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(day_number("2000-03-01"), Some(11_017));
        assert_eq!(day_number("2026-10-03"), Some(20_729));
        assert_eq!(day_number("2026-13-01"), None);
        assert_eq!(day_number("yesterday"), None);
    }

    #[test]
    fn a_search_is_words_and_modifiers() {
        let terms = parse_search("Release -draft from:@Lena in:development after:2026-10-01");
        assert_eq!(terms.words, ["release"]);
        assert_eq!(terms.excluded, ["draft"]);
        assert_eq!(terms.from, ["lena"]);
        assert_eq!(terms.channels, ["development"]);
        assert_eq!(terms.after, Some(20_727));
    }

    #[test]
    fn a_post_is_a_hit_when_every_part_of_the_search_agrees() {
        let noon = 20_729_i64 * 86_400_000 + 12 * 3_600_000;
        let post = json!({"message": "Release notes draft is up", "user_id": LENA,
                          "channel_id": DEV, "create_at": noon});
        let hit = |terms: &str| parse_search(terms).matches(&post, false, 0);
        assert!(hit("release notes"));
        assert!(!hit("release tomorrow"));
        assert!(parse_search("release tomorrow").matches(&post, true, 0));
        assert!(hit("from:lena in:development"));
        assert!(!hit("from:mikk"));
        assert!(!hit("in:general"));
        assert!(!hit("notes -draft"));
        assert!(hit("on:2026-10-03"));
        assert!(hit("after:2026-10-02 before:2026-10-04"));
        assert!(!hit("before:2026-10-03"));
        // Half a day east of Greenwich, noon is already tomorrow.
        assert!(parse_search("on:2026-10-04").matches(&post, false, 13 * 3600));
    }
}
