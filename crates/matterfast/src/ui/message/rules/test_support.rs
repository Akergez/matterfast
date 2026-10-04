//! Fixtures shared by the rule tests.

use mattermost_api::models::Post;

use crate::state::AppState;

pub(crate) fn post_with(post_type: &str, message: &str, props: &[(&str, &str)]) -> Post {
    Post {
        r#type: post_type.to_string(),
        message: message.to_string(),
        props: props
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
            .collect(),
        ..Default::default()
    }
}

pub(crate) fn state_with(username: &str, first: &str, last: &str) -> AppState {
    let client = mattermost_api::Client::new("http://x.test").unwrap();
    let mut st = AppState::new(
        client,
        Default::default(),
        mattermost_api::models::ClientConfig::default(),
        false,
    );
    st.config
        .0
        .insert("TeammateNameDisplay".into(), "full_name".into());
    st.users.insert(
        "u1".into(),
        mattermost_api::models::User {
            id: "u1".into(),
            username: username.into(),
            first_name: first.into(),
            last_name: last.into(),
            ..Default::default()
        },
    );
    st
}
