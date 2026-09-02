//! The Agents plugin (`mattermost-ai`) — Mattermost's LLM module.
//!
//! Two things are worth knowing before reading this. First, there is no HTTP
//! route that starts a conversation: you talk to a bot by posting to its DM
//! channel, or by @-mentioning it, through the ordinary post API. The plugin
//! watches for that. Second, everything that "returns" an answer — summarise a
//! thread, summarise unreads — returns only the id of a post, and the answer
//! is *streamed into that post* afterwards.
//!
//! The streaming arrives as a `postupdate` websocket event whose `next` field
//! is the **whole message so far**, not a delta. Appending it would double
//! every character.

use serde::Deserialize;

use mattermost_api::{Client, Result};

/// The plugin's id, which is also its URL prefix.
pub const PLUGIN_ID: &str = "mattermost-ai";

/// Websocket events from the plugin arrive with this prefix.
pub const WS_PREFIX: &str = "custom_mattermost-ai_";

#[derive(Debug, Clone, Deserialize)]
/// The server puts the default bot first, so position carries that and no
/// flag is needed for it.
pub struct Bot {
    pub id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
    #[serde(default)]
    pub username: String,
    /// Empty when the DM channel does not exist yet — create it the normal way
    /// before posting.
    #[serde(rename = "dmChannelID", default)]
    pub dm_channel_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Bots {
    #[serde(default)]
    pub bots: Vec<Bot>,
}

/// `GET /plugins/mattermost-ai/ai_bots`.
///
/// Also the capability check: a server without the plugin 404s here, which is
/// how the UI knows not to offer any of this.
pub async fn bots(client: &Client) -> Result<Bots> {
    client
        .get_url(&client.plugin_url(PLUGIN_ID, "/ai_bots"), "ai bots")
        .await
}

/// What a streamed answer post is being told, parsed out of a `postupdate`
/// event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamUpdate {
    /// Replace the post's text with this. Cumulative, never a delta.
    Text { post_id: String, message: String },
    /// The answer is finished, or was cancelled.
    Done { post_id: String },
    /// A control frame we do not render (reasoning, tool calls, annotations).
    Ignored,
}

/// Reads a `postupdate` payload.
pub fn parse_stream(data: &mattermost_api::ws::Data) -> StreamUpdate {
    let string = |key: &str| {
        data.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    let post_id = string("post_id").to_string();
    if post_id.is_empty() {
        return StreamUpdate::Ignored;
    }
    match string("control") {
        // "start" and "continue" carry no text; the first `next` does.
        "" => match data.get("next").and_then(serde_json::Value::as_str) {
            Some(message) => StreamUpdate::Text {
                post_id,
                message: message.to_string(),
            },
            None => StreamUpdate::Ignored,
        },
        "end" | "cancel" => StreamUpdate::Done { post_id },
        _ => StreamUpdate::Ignored,
    }
}

/// `POST /plugins/mattermost-ai/post/{id}/analyze` — summarise a thread.
///
/// The response only names the post the answer will be written into; the text
/// arrives over the websocket.
pub async fn summarise_thread(client: &Client, post_id: &str) -> Result<AnalysisTarget> {
    let body = serde_json::json!({ "analysis_type": "summarize_thread" });
    client
        .post_url(
            &client.plugin_url(PLUGIN_ID, &format!("/post/{post_id}/analyze")),
            Some(&body),
            "summarise thread",
        )
        .await
}

/// `POST /plugins/mattermost-ai/channel/{id}/interval` — catch me up.
pub async fn summarise_unreads(client: &Client, channel_id: &str) -> Result<AnalysisTarget> {
    let body = serde_json::json!({
        "preset_prompt": "summarize_unreads",
        // 0 means "up to now"; the server caps the range at fourteen days.
        "start_time": 0,
        "end_time": 0,
    });
    client
        .post_url(
            &client.plugin_url(PLUGIN_ID, &format!("/channel/{channel_id}/interval")),
            Some(&body),
            "summarise unreads",
        )
        .await
}

/// Where an answer will appear.
#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisTarget {
    #[serde(rename = "postid")]
    pub post_id: String,
    #[serde(rename = "channelid")]
    pub channel_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(json: serde_json::Value) -> mattermost_api::ws::Data {
        match json {
            serde_json::Value::Object(map) => map,
            _ => unreachable!(),
        }
    }

    #[test]
    fn text_frames_carry_the_whole_message() {
        let update = parse_stream(&data(serde_json::json!({
            "post_id": "p1",
            "next": "Hello there",
        })));
        assert_eq!(
            update,
            StreamUpdate::Text {
                post_id: "p1".into(),
                message: "Hello there".into()
            }
        );
    }

    #[test]
    fn control_frames_are_start_end_or_noise() {
        assert_eq!(
            parse_stream(&data(
                serde_json::json!({"post_id": "p1", "control": "end"})
            )),
            StreamUpdate::Done {
                post_id: "p1".into()
            }
        );
        // Reasoning and tool traffic is not something to render as the answer.
        assert_eq!(
            parse_stream(&data(serde_json::json!({
                "post_id": "p1",
                "control": "tool_call",
                "tool_call": "{}"
            }))),
            StreamUpdate::Ignored
        );
        assert_eq!(
            parse_stream(&data(serde_json::json!({"next": "orphan"}))),
            StreamUpdate::Ignored
        );
    }
}
