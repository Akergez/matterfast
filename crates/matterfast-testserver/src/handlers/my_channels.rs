use axum::Json;
use serde_json::Value;

use crate::constants::{DEV, DM_LENA, GENERAL};
use crate::model::{channel, filler_count, filler_id};

pub(crate) async fn my_channels() -> Json<Value> {
    let mut out = vec![channel(GENERAL), channel(DEV), channel(DM_LENA)];
    out.extend((0..filler_count()).map(|n| channel(&filler_id(n))));
    Json(Value::Array(out))
}
