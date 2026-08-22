//! Data-channel signalling codec (`calls-dc`) and the zlib envelope shared with
//! the websocket SDP path.
//!
//! # Framing
//!
//! Unusually, a DC message is **not** a msgpack container. It is a msgpack
//! integer holding the type, followed immediately by an optional msgpack
//! payload, concatenated:
//!
//! ```text
//! [type: msgpack uint][payload: msgpack]?
//! ```
//!
//! Both spellings of that integer occur on the wire. Every type value is ≤ 9,
//! so it fits a positive fixint — one byte equal to the value, which is what we
//! send — but the plugin's Go encoder writes an explicit `uint8`, `0xcc` then
//! the value. A `Pong` from the SFU is therefore `cc 02`, not `02`, and a
//! decoder that only understands the compact form rejects every message the
//! server sends.

use std::collections::HashMap;
use std::io::{Read, Write};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::error::{CallsError, Result};
use crate::protocol::SDP_MAX_DECOMPRESSED;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DcType {
    Ping = 1,
    Pong = 2,
    Sdp = 3,
    LossRate = 4,
    RoundTripTime = 5,
    Jitter = 6,
    Lock = 7,
    Unlock = 8,
    MediaMap = 9,
}

impl DcType {
    pub fn from_u8(v: u8) -> Option<DcType> {
        Some(match v {
            1 => DcType::Ping,
            2 => DcType::Pong,
            3 => DcType::Sdp,
            4 => DcType::LossRate,
            5 => DcType::RoundTripTime,
            6 => DcType::Jitter,
            7 => DcType::Lock,
            8 => DcType::Unlock,
            9 => DcType::MediaMap,
            _ => return None,
        })
    }
}

/// One entry of the `mid → track` hint the SFU sends before each of its offers.
///
/// **`sender_id` is unreliable** — the SFU populates it with the *receiving*
/// session's id, not the sender's (a bug in `rtc/session.go`). No shipping
/// client reads it. Use [`crate::protocol::parse_track_id`] on the track id
/// instead; treat this map purely as a `mid → type` hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    #[serde(rename = "type")]
    pub track_type: String,
    #[serde(default, rename = "sender_id")]
    pub sender_id: String,
}

pub type MediaMap = HashMap<String, TrackInfo>;

/// A decoded data-channel message.
#[derive(Debug, Clone)]
pub enum DcMessage {
    Ping,
    Pong,
    /// Raw JSON of a `SessionDescription`, already decompressed.
    Sdp(String),
    LossRate(f64),
    /// Seconds, not milliseconds.
    RoundTripTime(f64),
    Jitter(f64),
    /// Request (no payload) or response (`Some(granted)`).
    Lock(Option<bool>),
    Unlock,
    MediaMap(MediaMap),
}

/// zlib-compresses (RFC 1950 — *not* raw deflate; the header and Adler-32
/// checksum are required).
pub fn zlib_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

/// zlib-decompresses, refusing anything that would exceed the plugin's own cap.
pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    ZlibDecoder::new(data)
        .take(SDP_MAX_DECOMPRESSED as u64 + 1)
        .read_to_end(&mut out)?;
    if out.len() > SDP_MAX_DECOMPRESSED {
        return Err(CallsError::Protocol(format!(
            "decompressed SDP exceeds {SDP_MAX_DECOMPRESSED} bytes"
        )));
    }
    Ok(out)
}

/// Encodes a data-channel message.
pub fn encode(msg: &DcMessage) -> Result<Vec<u8>> {
    let (ty, payload): (DcType, Option<Vec<u8>>) = match msg {
        DcMessage::Ping => (DcType::Ping, None),
        DcMessage::Pong => (DcType::Pong, None),
        DcMessage::Unlock => (DcType::Unlock, None),
        DcMessage::Lock(None) => (DcType::Lock, None),
        DcMessage::Lock(Some(granted)) => (DcType::Lock, Some(rmp_serde::to_vec(granted)?)),
        DcMessage::Sdp(json) => {
            let compressed = zlib_compress(json.as_bytes())?;
            // msgpack `bin`, which is what serde_bytes emits.
            (
                DcType::Sdp,
                Some(rmp_serde::to_vec(serde_bytes::Bytes::new(&compressed))?),
            )
        }
        DcMessage::LossRate(v) => (DcType::LossRate, Some(rmp_serde::to_vec(v)?)),
        DcMessage::RoundTripTime(v) => (DcType::RoundTripTime, Some(rmp_serde::to_vec(v)?)),
        DcMessage::Jitter(v) => (DcType::Jitter, Some(rmp_serde::to_vec(v)?)),
        DcMessage::MediaMap(m) => (DcType::MediaMap, Some(rmp_serde::to_vec_named(m)?)),
    };

    let mut out = Vec::with_capacity(1 + payload.as_ref().map_or(0, Vec::len));
    out.push(ty as u8);
    if let Some(p) = payload {
        out.extend_from_slice(&p);
    }
    Ok(out)
}

/// Decodes a data-channel message.
pub fn decode(data: &[u8]) -> Result<DcMessage> {
    let (&first, rest) = data
        .split_first()
        .ok_or_else(|| CallsError::Protocol("empty data channel message".into()))?;
    // `0xcc` is msgpack's uint8 marker: the value is the byte after it.
    let (value, rest) = if first == 0xcc {
        let (&v, rest) = rest
            .split_first()
            .ok_or_else(|| CallsError::Protocol("truncated data channel message type".into()))?;
        (v, rest)
    } else {
        (first, rest)
    };
    let ty = DcType::from_u8(value)
        .ok_or_else(|| CallsError::Protocol(format!("unknown dc message type {value}")))?;

    Ok(match ty {
        DcType::Ping => DcMessage::Ping,
        DcType::Pong => DcMessage::Pong,
        DcType::Unlock => DcMessage::Unlock,
        DcType::Lock => {
            // The request carries no payload; the response carries a bool.
            DcMessage::Lock(rmp_serde::from_slice::<bool>(rest).ok())
        }
        DcType::Sdp => {
            let packed: serde_bytes::ByteBuf = rmp_serde::from_slice(rest)?;
            let json = zlib_decompress(&packed)?;
            DcMessage::Sdp(String::from_utf8(json).map_err(|e| {
                CallsError::Protocol(format!("SDP payload was not valid UTF-8: {e}"))
            })?)
        }
        DcType::LossRate => DcMessage::LossRate(rmp_serde::from_slice(rest)?),
        DcType::RoundTripTime => DcMessage::RoundTripTime(rmp_serde::from_slice(rest)?),
        DcType::Jitter => DcMessage::Jitter(rmp_serde::from_slice(rest)?),
        DcType::MediaMap => DcMessage::MediaMap(rmp_serde::from_slice(rest)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_is_exactly_one_byte() {
        assert_eq!(encode(&DcMessage::Ping).unwrap(), vec![1u8]);
        assert_eq!(encode(&DcMessage::Pong).unwrap(), vec![2u8]);
        assert_eq!(encode(&DcMessage::Unlock).unwrap(), vec![8u8]);
        // A lock *request* has no payload either.
        assert_eq!(encode(&DcMessage::Lock(None)).unwrap(), vec![7u8]);
    }

    #[test]
    fn lock_response_appends_a_msgpack_bool() {
        let granted = encode(&DcMessage::Lock(Some(true))).unwrap();
        assert_eq!(granted, vec![7u8, 0xc3]);
        let denied = encode(&DcMessage::Lock(Some(false))).unwrap();
        assert_eq!(denied, vec![7u8, 0xc2]);

        match decode(&granted).unwrap() {
            DcMessage::Lock(Some(true)) => {}
            other => panic!("expected Lock(Some(true)), got {other:?}"),
        }
        // The plugin spells the type as an explicit msgpack uint8.
        match decode(&[0xcc, 7u8, 0xc3]).unwrap() {
            DcMessage::Lock(Some(true)) => {}
            other => panic!("expected Lock(Some(true)) from the go spelling, got {other:?}"),
        }
        match decode(&[0xcc, 2u8]).unwrap() {
            DcMessage::Pong => {}
            other => panic!("expected Pong from the go spelling, got {other:?}"),
        }
        match decode(&[7u8]).unwrap() {
            DcMessage::Lock(None) => {}
            other => panic!("expected Lock(None), got {other:?}"),
        }
    }

    #[test]
    fn sdp_round_trips_through_zlib_and_msgpack() {
        let json = r#"{"type":"offer","sdp":"v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n"}"#;
        let encoded = encode(&DcMessage::Sdp(json.into())).unwrap();
        assert_eq!(encoded[0], DcType::Sdp as u8);
        // Compression must actually happen, and it must be zlib (0x78 header).
        assert_ne!(encoded[1], 0x78, "payload must be msgpack-wrapped, not raw");

        match decode(&encoded).unwrap() {
            DcMessage::Sdp(out) => assert_eq!(out, json),
            other => panic!("expected Sdp, got {other:?}"),
        }
    }

    #[test]
    fn zlib_envelope_has_the_rfc1950_header() {
        let z = zlib_compress(b"hello hello hello hello").unwrap();
        // 0x78 is the zlib CMF byte; raw deflate would not have it.
        assert_eq!(z[0], 0x78, "must be zlib, not raw deflate");
        assert_eq!(zlib_decompress(&z).unwrap(), b"hello hello hello hello");
    }

    #[test]
    fn floats_round_trip() {
        for msg in [
            DcMessage::LossRate(0.125),
            DcMessage::RoundTripTime(0.042),
            DcMessage::Jitter(0.003),
        ] {
            let bytes = encode(&msg).unwrap();
            let back = decode(&bytes).unwrap();
            match (&msg, &back) {
                (DcMessage::LossRate(a), DcMessage::LossRate(b))
                | (DcMessage::RoundTripTime(a), DcMessage::RoundTripTime(b))
                | (DcMessage::Jitter(a), DcMessage::Jitter(b)) => {
                    assert!((a - b).abs() < f64::EPSILON)
                }
                _ => panic!("type changed across round trip"),
            }
        }
    }

    #[test]
    fn media_map_round_trips() {
        let mut m = MediaMap::new();
        m.insert(
            "1".into(),
            TrackInfo {
                track_type: "voice".into(),
                sender_id: "s1".into(),
            },
        );
        let bytes = encode(&DcMessage::MediaMap(m)).unwrap();
        match decode(&bytes).unwrap() {
            DcMessage::MediaMap(out) => {
                assert_eq!(out.get("1").unwrap().track_type, "voice");
            }
            other => panic!("expected MediaMap, got {other:?}"),
        }
    }

    #[test]
    fn unknown_and_empty_messages_are_errors_not_panics() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[99]).is_err());
    }
}
