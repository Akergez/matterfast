use serde_json::{json, Value};

use crate::constants::{LENA, MIKK, OLGA, SARA};

pub(crate) fn user(user_id: &str) -> Value {
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
