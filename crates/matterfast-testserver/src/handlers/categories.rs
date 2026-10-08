use axum::Json;
use serde_json::{json, Value};

use crate::constants::{DEV, DM_LENA, GENERAL, ME, TEAM};
use crate::model::{filler_count, filler_id};

pub(crate) async fn categories() -> Json<Value> {
    let mut channels = vec![GENERAL.to_string(), DEV.to_string()];
    channels.extend((0..filler_count()).map(filler_id));
    let mut categories = json!({
        "categories": [
            {"id": "channels_cat", "user_id": ME, "team_id": TEAM, "sort_order": 10,
             "sorting": "alpha", "type": "channels", "display_name": "Channels",
             "muted": false, "collapsed": false, "channel_ids": channels},
            {"id": "dm_cat", "user_id": ME, "team_id": TEAM, "sort_order": 20,
             "sorting": "recent", "type": "direct_messages", "display_name": "Direct Messages",
             "muted": false, "collapsed": false, "channel_ids": [DM_LENA]}
        ],
        "order": ["channels_cat", "dm_cat"]
    });
    // A favourite channel for live inbox-ordering scenarios.
    if std::env::var_os("MM_FAVORITES").is_some() {
        categories["categories"].as_array_mut().unwrap().insert(0, json!({
            "id": "fav_cat", "user_id": ME, "team_id": TEAM, "sort_order": 0,
            "sorting": "recent", "type": "favorites", "display_name": "Favorites",
            "muted": false, "collapsed": false, "channel_ids": [DEV]
        }));
        categories["order"].as_array_mut().unwrap().insert(0, json!("fav_cat"));
    }
    Json(categories)
}
