//! The WebRTC peer: media engine setup, negotiation and track attribution.
//!
//! # Codec ceiling
//!
//! The SFU's media engine registers exactly three codecs — Opus/111, VP8/96 and
//! AV1/45. No H.264, no VP9, no telephone-event. We register the same set (and
//! nothing else) so the SDP is small and the negotiation cannot land on
//! something the SFU will drop.
//!
//! # Header extensions
//!
//! `urn:ietf:params:rtp-hdrext:ssrc-audio-level` is **not optional**. Voice
//! activity is computed server-side from that extension, so without it you are
//! audible but never light up as "speaking" for anyone.

use std::sync::Arc;

use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTCRtpHeaderExtensionCapability, RTPCodecType,
};

use crate::config::IceServerConfig;
use crate::error::Result;
use crate::protocol::SessionDescription;

/// Opus, exactly as the SFU registers it.
pub const OPUS_PAYLOAD_TYPE: u8 = 111;
pub const OPUS_CLOCK_RATE: u32 = 48_000;
pub const OPUS_CHANNELS: u16 = 2;
pub const OPUS_FMTP: &str = "minptime=10;useinbandfec=1";

pub const VP8_PAYLOAD_TYPE: u8 = 96;
pub const AV1_PAYLOAD_TYPE: u8 = 45;

pub const AUDIO_LEVEL_EXT: &str = "urn:ietf:params:rtp-hdrext:ssrc-audio-level";
pub const MID_EXT: &str = "urn:ietf:params:rtp-hdrext:sdes:mid";
pub const RID_EXT: &str = "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id";
pub const RRID_EXT: &str = "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id";

/// Builds a media engine carrying only what the SFU understands.
pub fn media_engine(enable_av1: bool) -> Result<MediaEngine> {
    let mut m = MediaEngine::default();

    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: "audio/opus".to_owned(),
                clock_rate: OPUS_CLOCK_RATE,
                channels: OPUS_CHANNELS,
                sdp_fmtp_line: OPUS_FMTP.to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: OPUS_PAYLOAD_TYPE,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;

    let video_feedback = vec![
        webrtc::rtp_transceiver::RTCPFeedback {
            typ: "goog-remb".to_owned(),
            parameter: String::new(),
        },
        webrtc::rtp_transceiver::RTCPFeedback {
            typ: "ccm".to_owned(),
            parameter: "fir".to_owned(),
        },
        webrtc::rtp_transceiver::RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: String::new(),
        },
        webrtc::rtp_transceiver::RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "pli".to_owned(),
        },
    ];

    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: "video/VP8".to_owned(),
                clock_rate: 90_000,
                channels: 0,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: video_feedback.clone(),
            },
            payload_type: VP8_PAYLOAD_TYPE,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;

    if enable_av1 {
        m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/AV1".to_owned(),
                    clock_rate: 90_000,
                    channels: 0,
                    sdp_fmtp_line: String::new(),
                    rtcp_feedback: video_feedback,
                },
                payload_type: AV1_PAYLOAD_TYPE,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;
    }

    // Required for server-side voice activity detection.
    m.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: AUDIO_LEVEL_EXT.to_owned(),
        },
        RTPCodecType::Audio,
        None,
    )?;
    // Required for simulcast screen share.
    for uri in [MID_EXT, RID_EXT, RRID_EXT] {
        m.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: uri.to_owned(),
            },
            RTPCodecType::Video,
            None,
        )?;
    }

    Ok(m)
}

/// Builds the peer connection with the settings the reference client uses.
pub async fn peer_connection(
    ice_servers: &[IceServerConfig],
    enable_av1: bool,
) -> Result<Arc<RTCPeerConnection>> {
    mattermost_api::tls::install_crypto_provider();

    let mut m = media_engine(enable_av1)?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut m)?;

    // The SFU calls pion's `EnableSCTPZeroChecksum(true)`, which has no
    // equivalent here — and does not need one. RFC 9653 zero checksums are
    // *negotiated*: pion only omits the checksum if the peer advertised "Zero
    // Checksum Acceptable" in its INIT. webrtc-rs never advertises it, so the
    // SFU falls back to real CRC32c and the data channel works. (Worth
    // rechecking if webrtc-rs ever starts advertising it, because
    // `webrtc-sctp` rejects any packet whose checksum does not verify.)
    let se = SettingEngine::default();

    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .with_setting_engine(se)
        .build();

    let config = RTCConfiguration {
        ice_servers: ice_servers.iter().map(to_ice_server).collect(),
        ..Default::default()
    };

    Ok(Arc::new(api.new_peer_connection(config).await?))
}

fn to_ice_server(c: &IceServerConfig) -> RTCIceServer {
    RTCIceServer {
        urls: c.urls.clone(),
        username: c.username.clone(),
        credential: c.credential.clone(),
    }
}

/// Converts our wire type into webrtc-rs's session description.
pub fn to_rtc_sdp(sdp: &SessionDescription) -> Result<RTCSessionDescription> {
    Ok(match sdp.kind.as_str() {
        "offer" => RTCSessionDescription::offer(sdp.sdp.clone())?,
        "answer" => RTCSessionDescription::answer(sdp.sdp.clone())?,
        "pranswer" => RTCSessionDescription::pranswer(sdp.sdp.clone())?,
        other => {
            return Err(crate::error::CallsError::Protocol(format!(
                "unsupported sdp type {other:?}"
            )))
        }
    })
}

/// Converts webrtc-rs's session description into our wire type.
pub fn from_rtc_sdp(sdp: &RTCSessionDescription) -> SessionDescription {
    let kind = match sdp.sdp_type {
        RTCSdpType::Offer => "offer",
        RTCSdpType::Answer => "answer",
        RTCSdpType::Pranswer => "pranswer",
        RTCSdpType::Rollback => "rollback",
        RTCSdpType::Unspecified => "offer",
    };
    SessionDescription {
        kind: kind.to_owned(),
        sdp: sdp.sdp.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_engine_registers_the_sfu_codec_set() {
        // Building it at all proves the payload types do not collide, which is
        // the failure mode when a default codec set is left registered.
        assert!(media_engine(false).is_ok());
        assert!(media_engine(true).is_ok());
    }

    /// webrtc-rs parses the SDP eagerly, so a stub body will not do.
    const MINIMAL_SDP: &str = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n";

    #[test]
    fn sdp_types_round_trip() {
        for kind in ["offer", "answer", "pranswer"] {
            let wire = SessionDescription {
                kind: kind.into(),
                sdp: MINIMAL_SDP.into(),
            };
            let rtc = to_rtc_sdp(&wire).unwrap();
            let back = from_rtc_sdp(&rtc);
            assert_eq!(back.kind, kind);
            assert_eq!(back.sdp, MINIMAL_SDP);
        }
    }

    #[test]
    fn unknown_sdp_types_are_rejected() {
        let wire = SessionDescription {
            kind: "nonsense".into(),
            sdp: String::new(),
        };
        assert!(to_rtc_sdp(&wire).is_err());
    }
}
