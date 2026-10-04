use axum::Json;
use serde_json::Value;

use crate::constants::{DEV, DM_LENA, GENERAL};
use crate::model::{filler_count, filler_id, membership};

pub(crate) async fn my_channel_members() -> Json<Value> {
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
