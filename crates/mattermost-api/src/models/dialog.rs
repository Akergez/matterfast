//! Interactive dialogs (`server/public/model/integration_action.go`).
//!
//! A slash command or plugin asks the server to open a form; the server signs
//! the trigger and pushes an `open_dialog` websocket event at the one user who
//! triggered it. The client renders the form and POSTs the answers to
//! `/api/v4/actions/dialogs/submit`, which forwards them to the integration's
//! own URL.
//!
//! The one trap worth stating up front: the websocket event does **not** carry
//! the dialog as a nested object. `app.OpenInteractiveDialog` does
//! `message.Add("dialog", string(jsonRequest))`, so `data.dialog` is a *string*
//! holding a JSON-encoded [`OpenDialogRequest`] — the whole envelope, trigger
//! id and integration URL included, not just the [`Dialog`]. The webapp parses
//! it the same way (`JSON.parse(msg.data.dialog)` in `websocket_actions.ts`).
//! [`OpenDialogRequest::from_ws_data`] does that second decode.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

/// The `open_dialog` websocket payload, once the inner string is decoded.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OpenDialogRequest {
    /// Signed and re-issued by the server; echo it back nowhere — it is only
    /// used to match the event to the command the user just ran.
    #[serde(default)]
    pub trigger_id: String,
    /// The integration's own endpoint. Send it back as
    /// [`SubmitDialogRequest::url`]; the API refuses a submission without it.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub dialog: Dialog,
}

impl OpenDialogRequest {
    /// Decode the JSON string found under `data.dialog` of an `open_dialog`
    /// event.
    pub fn from_ws_data(dialog: &str) -> Result<OpenDialogRequest, serde_json::Error> {
        serde_json::from_str(dialog)
    }
}

/// `model.Dialog`. Every field is optional except the title.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Dialog {
    /// Opaque to us; hand it back unchanged on submit.
    #[serde(default)]
    pub callback_id: String,
    /// Server caps this at 24 characters.
    #[serde(default)]
    pub title: String,
    /// Markdown, shown above the fields.
    #[serde(default)]
    pub introduction_text: String,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub elements: Vec<DialogElement>,
    /// Label for the confirm button; empty means "Submit".
    #[serde(default)]
    pub submit_label: String,
    /// When set, dismissing the form should still POST, with
    /// [`SubmitDialogRequest::cancelled`] true.
    #[serde(default)]
    pub notify_on_cancel: bool,
    /// Opaque to us as well; hand it back unchanged.
    #[serde(default)]
    pub state: String,
}

/// `model.DialogElement`.
///
/// [`Self::element_type`] is left as a string rather than an enum: the server
/// keeps adding types (`date`, `datetime`, `file`, `action_button` are all
/// newer than the original five) and an unknown one should degrade to a text
/// field, not fail the whole parse.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DialogElement {
    /// The label. Capped at 24 characters by the server.
    #[serde(default)]
    pub display_name: String,
    /// The submission key. This is what the integration reads.
    #[serde(default)]
    pub name: String,
    /// `text` | `textarea` | `select` | `radio` | `bool` (and newer ones).
    #[serde(rename = "type", default)]
    pub element_type: String,
    /// For `text`/`textarea`: `""` | `email` | `number` | `password` | `tel` |
    /// `url`. Advisory — the server does not enforce the format.
    #[serde(default)]
    pub subtype: String,
    /// Pre-filled value. For `bool` it is the string `"true"`/`"false"`; for
    /// `select`/`radio` it is an option *value*.
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub placeholder: String,
    /// Shown under the field.
    #[serde(default)]
    pub help_text: String,
    /// `false` means the field must be filled in before submitting.
    #[serde(default)]
    pub optional: bool,
    /// `0` means unset for both of these.
    #[serde(default)]
    pub min_length: i64,
    #[serde(default)]
    pub max_length: i64,
    /// `""` (use [`Self::options`]) | `users` | `channels` | `dynamic`.
    /// Anything but `""` means the options live on the server, not in the
    /// payload.
    #[serde(default)]
    pub data_source: String,
    /// Where a `dynamic` data source is looked up. Unused otherwise.
    #[serde(default)]
    pub data_source_url: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub options: Vec<DialogOption>,
    /// A `select` that takes several values; submitted comma-separated.
    #[serde(default)]
    pub multiselect: bool,
}

/// `model.PostActionOptions` — one entry of a `select` or `radio`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DialogOption {
    /// What the user reads.
    pub text: String,
    /// What gets submitted.
    pub value: String,
}

/// `POST /api/v4/actions/dialogs/submit`.
///
/// `user_id` and `team_id` are deliberately absent: the API handler overwrites
/// both from the session and from the channel it loads, so sending them is at
/// best noise.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SubmitDialogRequest {
    /// [`OpenDialogRequest::url`]. Required; the API rejects an empty one.
    pub url: String,
    pub callback_id: String,
    pub state: String,
    pub channel_id: String,
    /// Values keyed by [`DialogElement::name`]. The server types this as
    /// `map[string]any`, but every element serialises to a string (`"true"` /
    /// `"false"` for `bool`, comma-joined for a multiselect).
    pub submission: HashMap<String, String>,
    /// Set when the user dismissed a dialog with `notify_on_cancel`.
    pub cancelled: bool,
    /// Empty is fine — the server fills in `"dialog_submission"`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub r#type: String,
}

/// What the integration answers with, relayed verbatim by the API.
///
/// An `errors` map keyed by element name means the form should stay open with
/// those fields flagged; `error` is a whole-form message.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SubmitDialogResponse {
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub errors: HashMap<String, String>,
    /// `""` | `ok` | `form` | `navigate`.
    #[serde(default)]
    pub r#type: String,
    /// Present only for `type == "form"`: a follow-up dialog to show instead
    /// of closing.
    #[serde(default)]
    pub form: Option<Dialog>,
}

impl SubmitDialogResponse {
    /// Whether the form should stay open and show what went wrong.
    pub fn failed(&self) -> bool {
        !self.error.is_empty() || !self.errors.is_empty()
    }
}

/// Go marshals a nil slice as `null`, so `elements` and `options` arrive as
/// `null` rather than `[]` whenever an integration leaves them out.
fn null_as_empty<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example from `docs/develop/integrate/plugins/interactive-dialogs`,
    /// trimmed to the element types we render, wrapped the way the websocket
    /// actually delivers it: the dialog is a *string* inside `data`.
    const OPEN_DIALOG_EVENT: &str = r#"{
      "event": "open_dialog",
      "data": {
        "dialog": "{\"trigger_id\":\"nbt1dxzqwpn6by14sfs66ganhc\",\"url\":\"http://localhost:5000/dialog_submit\",\"dialog\":{\"callback_id\":\"somecallbackid\",\"title\":\"Test Title\",\"introduction_text\":\"Some **introduction** text\",\"icon_url\":\"https://mattermost.com/icon.png\",\"submit_label\":\"Do it\",\"notify_on_cancel\":true,\"state\":\"somestate\",\"elements\":[{\"display_name\":\"Display Name\",\"name\":\"realname\",\"type\":\"text\",\"subtype\":\"\",\"default\":\"default text\",\"placeholder\":\"placeholder\",\"help_text\":\"This a regular input.\",\"optional\":false,\"min_length\":0,\"max_length\":0,\"data_source\":\"\",\"options\":null},{\"display_name\":\"Password\",\"name\":\"somepassword\",\"type\":\"text\",\"subtype\":\"password\",\"default\":\"\",\"placeholder\":\"\",\"help_text\":\"\",\"optional\":true,\"min_length\":0,\"max_length\":0,\"data_source\":\"\",\"options\":null},{\"display_name\":\"Long Text Area\",\"name\":\"realnametextarea\",\"type\":\"textarea\",\"subtype\":\"\",\"default\":\"\",\"placeholder\":\"placeholder\",\"help_text\":\"\",\"optional\":true,\"min_length\":5,\"max_length\":100,\"data_source\":\"\",\"options\":null},{\"display_name\":\"User Selector\",\"name\":\"someuserselector\",\"type\":\"select\",\"subtype\":\"\",\"default\":\"\",\"placeholder\":\"Select a user...\",\"help_text\":\"\",\"optional\":false,\"min_length\":0,\"max_length\":0,\"data_source\":\"users\",\"options\":null},{\"display_name\":\"Option Selector\",\"name\":\"someoptionselector\",\"type\":\"select\",\"subtype\":\"\",\"default\":\"opt2\",\"placeholder\":\"Select an option...\",\"help_text\":\"\",\"optional\":false,\"min_length\":0,\"max_length\":0,\"data_source\":\"\",\"options\":[{\"text\":\"Option1\",\"value\":\"opt1\"},{\"text\":\"Option2\",\"value\":\"opt2\"}]},{\"display_name\":\"Radio Option Selector\",\"name\":\"someradiooptionselector\",\"type\":\"radio\",\"help_text\":\"Choose one\",\"default\":\"engineering\",\"options\":[{\"text\":\"Engineering\",\"value\":\"engineering\"},{\"text\":\"Sales\",\"value\":\"sales\"}]},{\"display_name\":\"Boolean Selector\",\"name\":\"boolean_input\",\"type\":\"bool\",\"placeholder\":\"Was this modal helpful?\",\"default\":\"true\",\"optional\":true,\"help_text\":\"This is the help text\"}]}}"
      }
    }"#;

    #[derive(Deserialize)]
    struct Event {
        data: EventData,
    }
    #[derive(Deserialize)]
    struct EventData {
        dialog: String,
    }

    #[test]
    fn parses_the_doubly_encoded_websocket_payload() {
        let event: Event = serde_json::from_str(OPEN_DIALOG_EVENT).unwrap();
        let open = OpenDialogRequest::from_ws_data(&event.data.dialog).unwrap();

        assert_eq!(open.trigger_id, "nbt1dxzqwpn6by14sfs66ganhc");
        assert_eq!(open.url, "http://localhost:5000/dialog_submit");

        let dialog = &open.dialog;
        assert_eq!(dialog.callback_id, "somecallbackid");
        assert_eq!(dialog.title, "Test Title");
        assert_eq!(dialog.submit_label, "Do it");
        assert!(dialog.notify_on_cancel);
        assert_eq!(dialog.state, "somestate");
        assert_eq!(dialog.elements.len(), 7);

        let text = &dialog.elements[0];
        assert_eq!(text.element_type, "text");
        assert_eq!(text.default, "default text");
        assert!(!text.optional);
        // "options": null, not [].
        assert!(text.options.is_empty());

        assert_eq!(dialog.elements[1].subtype, "password");
        assert_eq!(dialog.elements[2].max_length, 100);
        // A server-backed select carries no options of its own.
        assert_eq!(dialog.elements[3].data_source, "users");
        assert!(dialog.elements[3].options.is_empty());
        assert_eq!(dialog.elements[4].options[1].value, "opt2");
        assert_eq!(dialog.elements[5].element_type, "radio");
        assert_eq!(dialog.elements[6].default, "true");
        // Absent fields, not just empty ones.
        assert_eq!(dialog.elements[6].min_length, 0);
        assert!(!dialog.elements[6].multiselect);
    }

    #[test]
    fn submission_serialises_flat_strings() {
        let req = SubmitDialogRequest {
            url: "http://localhost:5000/dialog_submit".into(),
            callback_id: "somecallbackid".into(),
            state: "somestate".into(),
            channel_id: "channelid".into(),
            submission: HashMap::from([("boolean_input".to_string(), "true".to_string())]),
            ..Default::default()
        };
        let json: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert_eq!(json["submission"]["boolean_input"], "true");
        assert_eq!(json["cancelled"], false);
        // Left out so the server applies "dialog_submission".
        assert!(json.get("type").is_none());
    }

    #[test]
    fn response_errors_keep_the_form_open() {
        let resp: SubmitDialogResponse =
            serde_json::from_str(r#"{"errors":{"realname":"Too short"}}"#).unwrap();
        assert!(resp.failed());
        assert_eq!(resp.errors["realname"], "Too short");
        assert!(resp.form.is_none());

        let ok: SubmitDialogResponse = serde_json::from_str(r#"{"type":"ok"}"#).unwrap();
        assert!(!ok.failed());
    }
}
