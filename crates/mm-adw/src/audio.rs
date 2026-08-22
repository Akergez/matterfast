//! Live audio for a call: microphone → Opus → the SFU, and back out to the
//! speakers.
//!
//! Everything runs at 48 kHz mono in 20 ms frames, which is the only shape the
//! Mattermost SFU and every other client speak. The capture side downmixes
//! whatever the device gives us; the playback side sums all remote speakers
//! into one stream.
//!
//! The two `cpal` streams live on the GTK thread inside [`AudioIo`] — dropping
//! it stops both devices, which is exactly what leaving a call should do.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, Sample as _, SampleFormat, SizedSample, StreamConfig};
use mattermost_calls::{
    write_audio_sample, CallSession, Sample, TrackLocalStaticSample, TrackRemote,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::runtime;

const SAMPLE_RATE: u32 = 48_000;
/// 20 ms at 48 kHz.
const FRAME: usize = 960;
/// The most audio one Opus packet can decode to (120 ms).
const MAX_DECODED: usize = FRAME * 6;
/// ponytail: no jitter buffer — packets play in arrival order, and a queue that
/// runs more than 200 ms ahead is dropped rather than left to drift. Add a
/// reorder window if anyone reports choppy audio on a lossy link.
const MAX_QUEUE: usize = SAMPLE_RATE as usize / 5;

/// The sample formats the streams below can convert, best first. ALSA offers
/// every format it can emulate — including `u8` for a mono device — so the
/// choice has to be ours rather than "whatever came first".
const FORMATS: [SampleFormat; 3] = [SampleFormat::F32, SampleFormat::I16, SampleFormat::U16];

/// Decoded audio waiting for the output callback, one queue per speaker.
type Queues = Arc<Mutex<HashMap<String, VecDeque<f32>>>>;

/// A running capture/playback pair. Drop it to stop both.
pub struct AudioIo {
    _input: cpal::Stream,
    _output: cpal::Stream,
    queues: Queues,
    /// Set once the user unmutes; until then captured frames are discarded.
    track: Arc<Mutex<Option<Arc<TrackLocalStaticSample>>>>,
}

impl AudioIo {
    /// Opens the default microphone and speakers and starts encoding.
    pub fn start(session: Arc<CallSession>) -> Result<Self, String> {
        let host = cpal::default_host();
        let mic = host
            .default_input_device()
            .ok_or("no microphone is available")?;
        let speakers = host
            .default_output_device()
            .ok_or("no audio output is available")?;

        let (in_cfg, in_fmt) = config_at_48k(&mic, true)?;
        let (out_cfg, out_fmt) = config_at_48k(&speakers, false)?;

        let queues: Queues = Arc::default();
        let track: Arc<Mutex<Option<Arc<TrackLocalStaticSample>>>> = Arc::default();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let input = build_input(&mic, &in_cfg, in_fmt, tx)?;
        let output = build_output(&speakers, &out_cfg, out_fmt, queues.clone())?;
        input.play().map_err(|e| e.to_string())?;
        output.play().map_err(|e| e.to_string())?;

        runtime::runtime().spawn(encode_loop(session, track.clone(), rx));

        Ok(AudioIo {
            _input: input,
            _output: output,
            queues,
            track,
        })
    }

    /// Starts playing a remote participant's track.
    pub fn play(&self, session_id: String, track: Arc<TrackRemote>) {
        let queues = self.queues.clone();
        runtime::runtime().spawn(async move {
            let mut decoder = match opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(error = %e, "could not create an Opus decoder");
                    return;
                }
            };
            let mut pcm = vec![0f32; MAX_DECODED];
            let mut packets = 0u64;
            while let Ok((packet, _)) = track.read_rtp().await {
                if packet.payload.is_empty() {
                    continue;
                }
                let Ok(n) = decoder.decode_float(&packet.payload, &mut pcm, false) else {
                    continue;
                };
                let mut queues = queues.lock().unwrap();
                let queue = queues.entry(session_id.clone()).or_default();
                if queue.len() > MAX_QUEUE {
                    queue.clear();
                }
                queue.extend(&pcm[..n]);
                packets += 1;
                // One line a second is enough to see whether audio is flowing.
                if packets % 50 == 0 {
                    tracing::debug!(%session_id, packets, queued = queue.len(), "receiving");
                }
            }
            queues.lock().unwrap().remove(&session_id);
        });
    }

    /// Hands the capture loop the track to write into, after an unmute.
    pub fn set_track(&self, track: Arc<TrackLocalStaticSample>) {
        *self.track.lock().unwrap() = Some(track);
    }
}

/// Picks a device configuration that runs at exactly 48 kHz.
///
/// Resampling is the one thing this pipeline refuses to do: ALSA's `Nearest`
/// rate matching would silently hand us 44.1 kHz and every frame would go out
/// pitched up.
fn config_at_48k(device: &Device, input: bool) -> Result<(StreamConfig, SampleFormat), String> {
    let supported: Vec<_> = if input {
        device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .collect()
    } else {
        device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .collect()
    };

    supported
        .into_iter()
        .filter(|c| c.min_sample_rate() <= SAMPLE_RATE && SAMPLE_RATE <= c.max_sample_rate())
        .filter_map(|c| Some((FORMATS.iter().position(|f| *f == c.sample_format())?, c)))
        // Best format first, then fewest channels: we only ever need one, and
        // asking for eight on a surround card wastes a conversion.
        .min_by_key(|(rank, c)| (*rank, c.channels()))
        .map(|(_, c)| c)
        .map(|c| {
            let c = c.with_sample_rate(SAMPLE_RATE);
            (c.config(), c.sample_format())
        })
        .ok_or_else(|| {
            let what = if input { "microphone" } else { "audio output" };
            format!("the default {what} does not support 48 kHz")
        })
}

fn build_input(
    device: &Device,
    cfg: &StreamConfig,
    fmt: SampleFormat,
    tx: UnboundedSender<Vec<f32>>,
) -> Result<cpal::Stream, String> {
    fn make<T>(
        device: &Device,
        cfg: &StreamConfig,
        tx: UnboundedSender<Vec<f32>>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = cfg.channels as usize;
        device.build_input_stream::<T, _, _>(
            *cfg,
            move |data: &[T], _| {
                let mono = data
                    .chunks(channels)
                    .map(|frame| {
                        frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32
                    })
                    .collect();
                let _ = tx.send(mono);
            },
            |e| tracing::warn!(error = %e, "microphone stream error"),
            None,
        )
    }

    match fmt {
        SampleFormat::F32 => make::<f32>(device, cfg, tx),
        SampleFormat::I16 => make::<i16>(device, cfg, tx),
        SampleFormat::U16 => make::<u16>(device, cfg, tx),
        other => return Err(format!("unsupported capture format {other}")),
    }
    .map_err(|e| e.to_string())
}

fn build_output(
    device: &Device,
    cfg: &StreamConfig,
    fmt: SampleFormat,
    queues: Queues,
) -> Result<cpal::Stream, String> {
    fn make<T>(
        device: &Device,
        cfg: &StreamConfig,
        queues: Queues,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = cfg.channels as usize;
        device.build_output_stream::<T, _, _>(
            *cfg,
            move |data: &mut [T], _| {
                let mut queues = queues.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    let mixed: f32 = queues
                        .values_mut()
                        .filter_map(|q| q.pop_front())
                        .sum::<f32>()
                        .clamp(-1.0, 1.0);
                    let sample = T::from_sample(mixed);
                    frame.fill(sample);
                }
            },
            |e| tracing::warn!(error = %e, "playback stream error"),
            None,
        )
    }

    match fmt {
        SampleFormat::F32 => make::<f32>(device, cfg, queues),
        SampleFormat::I16 => make::<i16>(device, cfg, queues),
        SampleFormat::U16 => make::<u16>(device, cfg, queues),
        other => return Err(format!("unsupported playback format {other}")),
    }
    .map_err(|e| e.to_string())
}

/// Encodes captured audio into 20 ms Opus packets for as long as the channel
/// stays open — which is until [`AudioIo`] is dropped.
async fn encode_loop(
    session: Arc<CallSession>,
    track: Arc<Mutex<Option<Arc<TrackLocalStaticSample>>>>,
    mut rx: UnboundedReceiver<Vec<f32>>,
) {
    let mut encoder =
        match opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "could not create an Opus encoder");
                return;
            }
        };

    let mut pending: Vec<f32> = Vec::with_capacity(FRAME * 2);
    let mut frames = 0u64;
    while let Some(chunk) = rx.recv().await {
        pending.extend_from_slice(&chunk);
        while pending.len() >= FRAME {
            let frame: Vec<f32> = pending.drain(..FRAME).collect();
            // Muted means the SFU drops our RTP anyway; not encoding it saves
            // the CPU and keeps us off the "speaking" list for certain.
            if session.is_muted() {
                continue;
            }
            let Some(track) = track.lock().unwrap().clone() else {
                continue;
            };
            let Ok(data) = encoder.encode_vec_float(&frame, 4000) else {
                continue;
            };
            let sample = Sample {
                data: data.into(),
                duration: Duration::from_millis(20),
                ..Default::default()
            };
            let level = level_dbov(&frame);
            frames += 1;
            if frames % 50 == 0 {
                tracing::debug!(frames, bytes = sample.data.len(), level, "sending");
            }
            if let Err(e) = write_audio_sample(&track, &sample, level).await {
                tracing::warn!(error = %e, "could not send audio; stopping capture");
                return;
            }
        }
    }
}

/// RFC 6464 level of a frame: 0 is loudest, 127 is silence.
///
/// The SFU's voice detector watches the *variance* of this over a 50-packet
/// window, so it has to be measured per frame — a constant never registers as
/// speech, however loud it claims to be.
fn level_dbov(frame: &[f32]) -> u8 {
    let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
    if rms <= 1e-6 {
        return 127;
    }
    (-20.0 * rms.log10()).clamp(0.0, 127.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_runs_from_silence_to_full_scale() {
        assert_eq!(level_dbov(&[0.0; FRAME]), 127);
        // Full-scale square wave is 0 dBov by definition.
        assert_eq!(level_dbov(&[1.0; FRAME]), 0);
        // Quieter speech sits somewhere in between, and louder means smaller.
        let loud = level_dbov(&[0.5; FRAME]);
        let quiet = level_dbov(&[0.01; FRAME]);
        assert!(loud < quiet, "{loud} should be below {quiet}");
        assert!((1..127).contains(&quiet));
    }
}
