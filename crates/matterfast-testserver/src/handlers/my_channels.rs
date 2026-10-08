use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::Value;

use crate::app::App;
use crate::constants::{DEV, DM_LENA, GENERAL};
use crate::model::{channel, filler_count, filler_id};

pub(crate) async fn my_channels(State(app): State<Arc<App>>) -> Json<Value> {
    let mut out = vec![channel(GENERAL), channel(DEV), channel(DM_LENA)];
    out.extend((0..filler_count()).map(|n| channel(&filler_id(n))));

    // A channel was last written in when its newest post was made, to the
    // millisecond: the real server copies the post's own time. A client
    // that tells "there is something newer than what I hold" by comparing
    // the two is told the truth only if the fake keeps to that.
    let db = app.db.lock().unwrap();
    for channel in &mut out {
        let id = channel["id"].clone();
        let newest = |roots_only: bool| {
            db.posts
                .iter()
                .map(|post| &post.v)
                .filter(|post| post["channel_id"] == id)
                .filter(|post| {
                    !roots_only || post["root_id"].as_str().unwrap_or_default().is_empty()
                })
                .filter_map(|post| post["create_at"].as_i64())
                .max()
        };
        if let Some(written) = newest(false) {
            channel["last_post_at"] = written.into();
        }
        if let Some(written) = newest(true) {
            channel["last_root_post_at"] = written.into();
        }
    }
    drop(db);
    Json(Value::Array(out))
}
