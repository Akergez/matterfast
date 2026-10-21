use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::app::App;
use crate::constants::ME;

pub(crate) async fn my_preferences(State(app): State<Arc<App>>) -> Json<Value> {
    Json(Value::Array(app.db.lock().unwrap().preferences.clone()))
}

/// `PUT /users/{user}/preferences`: replaces what is held under each
/// category and name, and tells every connection, as the server does.
pub(crate) async fn save_preferences(
    State(app): State<Arc<App>>,
    Json(changed): Json<Vec<Value>>,
) -> Json<Value> {
    {
        let mut db = app.db.lock().unwrap();
        for preference in &changed {
            let same = |held: &Value| {
                held["category"] == preference["category"] && held["name"] == preference["name"]
            };
            match db.preferences.iter_mut().find(|held| same(held)) {
                Some(held) => *held = preference.clone(),
                None => db.preferences.push(preference.clone()),
            }
        }
    }
    app.emit(
        "preferences_changed",
        // A JSON string, not an array, like the rest of the server's payloads.
        json!({ "preferences": serde_json::to_string(&changed).unwrap() }),
        json!({ "user_id": ME }),
    );
    Json(json!({ "status": "OK" }))
}
