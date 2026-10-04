use axum::Json;
use serde_json::{json, Value};

pub(crate) async fn zed_extensions() -> Json<Value> {
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
