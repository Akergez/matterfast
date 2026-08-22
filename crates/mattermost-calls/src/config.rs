//! Discovery and the plugin's other REST routes.
//!
//! Base path is `{SiteURL}/plugins/com.mattermost.calls` — **not** under
//! `/api/v4`. Every route except `/version` needs a normal Mattermost session,
//! so we reuse [`mattermost_api::Client`]'s bearer auth.
//!
//! One surprise worth knowing: `GET /config` returns Go field names verbatim
//! (`ICEServersConfigs`, `AllowScreenSharing`, …) because the plugin's
//! configuration struct carries no `json` tags. The nested ICE server objects,
//! which come from a different package, *do* have lowercase tags.

use mattermost_api::Client;
use serde::{Deserialize, Deserializer};

/// `#[serde(default)]` only covers a *missing* field; the plugin sends explicit
/// `null` for lists it has never been configured with (`"ICEServers":null`).
fn null_as_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::deserialize(d)?.unwrap_or_default())
}

use crate::error::{CallsError, Result};
use crate::protocol::PLUGIN_ID;

/// `GET /plugins/com.mattermost.calls/version`. Unauthenticated.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub build: String,
    /// Empty when the plugin runs its SFU in-process rather than via `rtcd`.
    #[serde(default)]
    pub rtcd_version: String,
    #[serde(default)]
    pub rtcd_build: String,
}

impl VersionInfo {
    /// Whether the server supports the data-channel **signalling lock**.
    ///
    /// Requires plugin ≥ 1.7.0 and, when an external `rtcd` is in play, rtcd ≥
    /// 1.1.0. Without the lock, both sides can start a renegotiation at once
    /// and the SFU — which is the impolite peer — silently drops ours.
    pub fn supports_dc_locking(&self) -> bool {
        fn at_least(v: &str, major: u32, minor: u32) -> bool {
            // `master` and `dev*` builds count as newest.
            if v == "master" || v.starts_with("dev") || v.is_empty() {
                return true;
            }
            let v = v.trim_start_matches('v');
            // A pre-release sorts *below* its release under semver, and the
            // reference implementation uses `semver.gte` — so 1.7.0-rc1 does
            // not count as 1.7.0. Enabling locking against an rc that does not
            // implement it would make every `Lock` time out after 5 seconds.
            if v.contains('-') || v.contains('+') {
                return false;
            }
            let mut it = v.split('.');
            let a: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let b: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            (a, b) >= (major, minor)
        }
        if self.version.is_empty() {
            return false;
        }
        at_least(&self.version, 1, 7)
            && (self.rtcd_version.is_empty() || at_least(&self.rtcd_version, 1, 1))
    }
}

/// A STUN/TURN server. Lowercase tags here, unlike the enclosing config.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct IceServerConfig {
    #[serde(default, deserialize_with = "null_as_default")]
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub credential: String,
}

/// `GET /plugins/com.mattermost.calls/config`.
///
/// Field names are the Go struct field names, not snake_case — the plugin's
/// configuration struct has no `json` tags.
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(non_snake_case)]
pub struct CallsConfig {
    #[serde(default, rename = "ICEServers", deserialize_with = "null_as_default")]
    pub ice_servers: Vec<String>,
    #[serde(
        default,
        rename = "ICEServersConfigs",
        deserialize_with = "null_as_default"
    )]
    pub ice_servers_configs: Vec<IceServerConfig>,
    #[serde(default, rename = "AllowEnableCalls")]
    pub allow_enable_calls: Option<bool>,
    #[serde(default, rename = "DefaultEnabled")]
    pub default_enabled: Option<bool>,
    #[serde(default, rename = "MaxCallParticipants")]
    pub max_call_participants: Option<i64>,
    #[serde(default, rename = "NeedsTURNCredentials")]
    pub needs_turn_credentials: Option<bool>,
    #[serde(default, rename = "AllowScreenSharing")]
    pub allow_screen_sharing: Option<bool>,
    #[serde(default, rename = "EnableRecordings")]
    pub enable_recordings: Option<bool>,
    #[serde(default, rename = "EnableTranscriptions")]
    pub enable_transcriptions: Option<bool>,
    #[serde(default, rename = "EnableLiveCaptions")]
    pub enable_live_captions: Option<bool>,
    #[serde(default, rename = "EnableSimulcast")]
    pub enable_simulcast: Option<bool>,
    #[serde(default, rename = "EnableRinging")]
    pub enable_ringing: Option<bool>,
    #[serde(default)]
    pub sku_short_name: String,
    #[serde(default, rename = "HostControlsAllowed")]
    pub host_controls_allowed: bool,
    #[serde(default, rename = "EnableAV1")]
    pub enable_av1: Option<bool>,
    #[serde(default, rename = "GroupCallsAllowed")]
    pub group_calls_allowed: bool,
    #[serde(default, rename = "EnableDCSignaling")]
    pub enable_dc_signaling: Option<bool>,
    #[serde(default, rename = "EnableVideo")]
    pub enable_video: Option<bool>,
}

impl CallsConfig {
    pub fn screen_sharing_allowed(&self) -> bool {
        self.allow_screen_sharing.unwrap_or(false)
    }
    pub fn dc_signaling_allowed(&self) -> bool {
        self.enable_dc_signaling.unwrap_or(false)
    }
    pub fn av1_allowed(&self) -> bool {
        self.enable_av1.unwrap_or(false)
    }
    pub fn needs_turn(&self) -> bool {
        self.needs_turn_credentials.unwrap_or(false)
    }
    pub fn ringing_enabled(&self) -> bool {
        self.enable_ringing.unwrap_or(false)
    }
}

/// `GET /plugins/com.mattermost.calls/{channel_id}` — whether calls are enabled
/// in a channel and, if one is running, its state.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelCallState {
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub call: Option<crate::protocol::CallState>,
}

impl ChannelCallState {
    pub fn has_ongoing_call(&self) -> bool {
        self.call.is_some()
    }
}

/// Everything discovery yields, resolved into what the RTC layer needs.
#[derive(Debug, Clone)]
pub struct Discovery {
    pub version: VersionInfo,
    pub config: CallsConfig,
    /// `ice_servers_configs` plus any short-lived TURN credentials.
    pub ice_servers: Vec<IceServerConfig>,
    pub dc_locking: bool,
}

/// Runs the discovery prefix of the join sequence.
///
/// Steps 1, 2 and 4 of the documented join flow: version → config → TURN
/// credentials (only when the server says it mints them). All three must
/// complete before the peer connection is built, because the ICE server list is
/// an input to it.
pub async fn discover(client: &Client) -> Result<Discovery> {
    let version: VersionInfo = client
        .get_url(&client.plugin_url(PLUGIN_ID, "/version"), "calls version")
        .await
        .map_err(|e| CallsError::Unavailable(format!("plugin not reachable: {e}")))?;

    let config: CallsConfig = client
        .get_url(&client.plugin_url(PLUGIN_ID, "/config"), "calls config")
        .await?;

    let mut ice_servers = config.ice_servers_configs.clone();
    // Fall back to the legacy flat list when the structured one is empty.
    if ice_servers.is_empty() && !config.ice_servers.is_empty() {
        ice_servers.push(IceServerConfig {
            urls: config.ice_servers.clone(),
            ..Default::default()
        });
    }

    if config.needs_turn() {
        match client
            .get_url::<Vec<IceServerConfig>>(
                &client.plugin_url(PLUGIN_ID, "/turn-credentials"),
                "turn credentials",
            )
            .await
        {
            Ok(turn) => ice_servers.extend(turn),
            // A TURN failure is not fatal — a call may still connect over STUN.
            Err(e) => tracing::warn!(error = %e, "could not fetch TURN credentials"),
        }
    }

    let dc_locking = version.supports_dc_locking();

    Ok(Discovery {
        version,
        config,
        ice_servers,
        dc_locking,
    })
}

/// Fetches the call state of a single channel.
pub async fn channel_state(client: &Client, channel_id: &str) -> Result<ChannelCallState> {
    Ok(client
        .get_url(
            &client.plugin_url(PLUGIN_ID, &format!("/{channel_id}")),
            "channel call state",
        )
        .await?)
}

/// Fetches call state for every readable channel — what a sidebar needs to show
/// "call in progress" badges.
pub async fn all_channel_states(client: &Client) -> Result<Vec<ChannelCallState>> {
    Ok(client
        .get_url(&client.plugin_url(PLUGIN_ID, "/channels"), "call states")
        .await?)
}

/// Starts or stops the server-side recording of a channel's ongoing call.
///
/// The **channel** id, despite the route reading `/calls/{id}/`: the plugin's
/// own HTTP API resolves the ongoing call from the channel, and passing the
/// call id — which is what `rtcd`'s client library takes — is answered with a
/// flat `403 no call ongoing`.
pub async fn set_recording(client: &Client, channel_id: &str, on: bool) -> Result<()> {
    let verb = if on { "start" } else { "stop" };
    let url = client.plugin_url(PLUGIN_ID, &format!("/calls/{channel_id}/recording/{verb}"));
    // The response is the job state, which the `call_job_state` event repeats
    // to everyone anyway.
    let _: serde_json::Value = client
        .post_url(&url, Option::<&()>::None, "call recording")
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_locking_requires_both_plugin_and_rtcd_versions() {
        let v = |p: &str, r: &str| VersionInfo {
            version: p.into(),
            rtcd_version: r.into(),
            ..Default::default()
        };
        // Plugin new enough, no external rtcd.
        assert!(v("1.7.0", "").supports_dc_locking());
        assert!(v("2.3.1", "").supports_dc_locking());
        // Plugin too old.
        assert!(!v("1.6.9", "").supports_dc_locking());
        // Plugin fine but rtcd too old.
        assert!(!v("1.9.0", "1.0.5").supports_dc_locking());
        assert!(v("1.9.0", "1.1.0").supports_dc_locking());
        // Dev builds count as newest.
        assert!(v("master", "dev-abc").supports_dc_locking());
        // A pre-release is *below* its release, as `semver.gte` has it.
        assert!(!v("1.7.0-rc1", "").supports_dc_locking());
        assert!(v("1.7.0", "").supports_dc_locking());
        // No version at all → assume unsupported.
        assert!(!v("", "").supports_dc_locking());
    }

    #[test]
    fn config_uses_go_field_names_verbatim() {
        let raw = r#"{
            "ICEServers":["stun:stun.example.com:3478"],
            "ICEServersConfigs":[{"urls":["turn:t.example.com"],"username":"u","credential":"p"}],
            "AllowScreenSharing":true,
            "EnableDCSignaling":false,
            "EnableAV1":true,
            "MaxCallParticipants":8,
            "sku_short_name":"professional"
        }"#;
        let c: CallsConfig = serde_json::from_str(raw).unwrap();
        assert!(c.screen_sharing_allowed());
        assert!(!c.dc_signaling_allowed());
        assert!(c.av1_allowed());
        assert_eq!(c.max_call_participants, Some(8));
        assert_eq!(c.ice_servers_configs[0].username, "u");
        assert_eq!(c.sku_short_name, "professional");
    }

    #[test]
    fn channel_state_without_a_call_deserializes() {
        let c: ChannelCallState =
            serde_json::from_str(r#"{"channel_id":"c1","enabled":true}"#).unwrap();
        assert!(c.enabled);
        assert!(!c.has_ongoing_call());
    }

    #[test]
    fn config_tolerates_null_lists() {
        let c: CallsConfig =
            serde_json::from_str(r#"{"ICEServers":null,"ICEServersConfigs":null}"#).unwrap();
        assert!(c.ice_servers.is_empty());
        assert!(c.ice_servers_configs.is_empty());
    }
}
