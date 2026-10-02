//! Playing an attached video or audio file where it was posted.
//!
//! A [`Player`] starts as a poster — a still, or failing that just the name,
//! the size and a play button — and only downloads the *whole* file when
//! someone presses play, because a channel scrolled past ten videos would
//! otherwise fetch ten videos. The still itself costs far less: the server
//! makes no thumbnail for video (it 400s `no_thumbnail`, confirmed against a
//! live server), so one is built from just the head of the file — see
//! [`head_playable`] for how much that is and which containers it works for.
//!
//! The file itself is behind the session token, so a URL handed to GStreamer
//! would come back 401; the bytes arrive the same way an image's do, through
//! the client, and are written to a temporary file to play from.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, AnyElement, App, ObjectFit, RenderImage};
use mattermost_api::models::FileInfo;

use super::kit::{self, Lucide};
use super::Ui;
use crate::playback::{self, Playback};
use crate::runtime;

/// The still shown before a video is played, and the picture while it is.
/// Matches the size an image attachment is drawn at, so a channel of clips
/// and photos reads evenly.
const POSTER_WIDTH: f32 = 420.0;
const POSTER_HEIGHT: f32 = 260.0;

/// How often a playing file's position is read. It is only a label.
const PROGRESS_TICK: Duration = Duration::from_millis(500);

/// Containers and codecs a default GStreamer install usually cannot decode, so
/// offering a play button for them would only produce a black box.
///
/// Windows Media and RealMedia need `-ugly`/`-bad` demuxers plus `libav` for
/// the video; MPEG-1/2 program and transport streams need `mpeg2dec`; Flash
/// and DivX-flavoured AVI need `libav` decoders. All of those are packaged
/// separately from `gst-plugins-base`/`-good` on most distributions.
const UNDECODABLE_EXT: &[&str] = &[
    "asf", "wmv", "wma", "rm", "rmvb", "ram", "flv", "f4v", "mpg", "mpeg", "mpe", "m1v", "m2v",
    "vob", "ts", "m2ts", "mts", "avi", "divx", "mid", "midi",
];

/// The same list by mime type, for servers that fill it in.
const UNDECODABLE_MIME: &[&str] = &[
    "video/mpeg",
    "video/mp2t",
    "video/x-ms",
    "audio/x-ms",
    "video/x-flv",
    "video/x-msvideo",
    "video/vnd.rn-",
    "audio/vnd.rn-",
    "application/vnd.rn-",
    "audio/midi",
    "audio/x-midi",
];

/// What we play when the server leaves `mime_type` empty.
const VIDEO_EXT: &[&str] = &["mp4", "m4v", "mov", "webm", "mkv", "ogv", "3gp"];
const AUDIO_EXT: &[&str] = &[
    "mp3", "m4a", "aac", "flac", "wav", "ogg", "oga", "opus", "weba", "aiff",
];

fn extension(file: &FileInfo) -> String {
    file.extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

/// True when this file is something we can play inline.
pub fn is_playable(file: &FileInfo) -> bool {
    let ext = extension(file);
    if UNDECODABLE_EXT.contains(&ext.as_str()) {
        return false;
    }

    // The mime type is the server's own answer, so it wins; the extension is
    // only consulted when there is nothing to win against.
    let mime = file.mime_type.trim().to_ascii_lowercase();
    if !mime.is_empty() {
        if UNDECODABLE_MIME.iter().any(|bad| mime.starts_with(bad)) {
            return false;
        }
        return mime.starts_with("video/") || mime.starts_with("audio/");
    }

    VIDEO_EXT.contains(&ext.as_str()) || AUDIO_EXT.contains(&ext.as_str())
}

/// True for the ones that have nothing to show: audio gets controls, no poster.
pub fn is_audio(file: &FileInfo) -> bool {
    let mime = file.mime_type.trim().to_ascii_lowercase();
    if !mime.is_empty() {
        return mime.starts_with("audio/");
    }
    AUDIO_EXT.contains(&extension(file).as_str())
}

/// Whether `head` — the first slice of a file, not necessarily all of it —
/// contains enough of the container's own index to build a poster from
/// without fetching any more of the file.
///
/// WebM/Matroska (EBML) is always worth trying: both mux their content as a
/// sequence of clusters that a demuxer can start reading from the front, so
/// there is no index that has to arrive first. An ISO base media file
/// (MP4/MOV/M4V) is the opposite: nothing can be decoded until `moov` (the
/// sample table) is in hand, and an encoder that skips `-movflags
/// +faststart` writes `mdat` — the (large) media itself — before it, putting
/// `moov` at the very end of the file. [`mp4_head_has_full_moov`] is the
/// cheap check for that; failing it means giving up quietly rather than
/// fetching the rest of a multi-hundred-MB file for a thumbnail.
pub fn head_playable(head: &[u8]) -> bool {
    // The four bytes every EBML document — WebM included — starts with.
    const EBML_MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];
    head.starts_with(&EBML_MAGIC) || mp4_head_has_full_moov(head)
}

/// Walks the top-level ISO-BMFF boxes in `head` (`[u32 size][4cc type][...]`,
/// or a `u64` size when the 32-bit one reads as `1`) looking for a `moov`
/// whose declared size ends inside what was fetched.
///
/// For a non-faststart file this runs out of `head` while still inside the
/// leading `mdat` — which, being the whole video, is almost always bigger
/// than `head` itself — and returns `false` well before reaching the real
/// `moov` at the end. That is the giving-up case, not a bug: the caller does
/// not fetch further to find out for certain.
fn mp4_head_has_full_moov(head: &[u8]) -> bool {
    let mut pos = 0usize;
    while pos + 8 <= head.len() {
        let size32 = u32::from_be_bytes(head[pos..pos + 4].try_into().unwrap()) as u64;
        let kind = &head[pos + 4..pos + 8];

        let (header_len, size) = if size32 == 1 {
            // The real size is a 64-bit field right after the ordinary
            // header; a box needing one is rare enough that the head simply
            // not containing it is treated the same as not finding `moov`.
            if pos + 16 > head.len() {
                break;
            }
            (
                16u64,
                u64::from_be_bytes(head[pos + 8..pos + 16].try_into().unwrap()),
            )
        } else if size32 == 0 {
            // "Runs to the end of the file" — never true of a box we still
            // need to skip past to reach `moov` within a bounded head.
            break;
        } else {
            (8, size32)
        };

        if kind == b"moov" {
            return pos as u64 + size <= head.len() as u64;
        }
        if size < header_len {
            break; // malformed box: an infinite loop is worse than giving up
        }
        match pos.checked_add(size as usize) {
            Some(next) => pos = next,
            None => break,
        }
    }
    false
}

/// A file id or an extension as part of a file name: only what cannot mean
/// anything to a shell or a path.
fn sanitised(text: &str) -> String {
    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if clean.is_empty() {
        "bin".to_string()
    } else {
        clean
    }
}


/// What a player is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// A poster and a play button.
    Idle,
    /// The file is on its way.
    Loading,
    Playing,
    Paused,
    /// It reached the end; play starts it over.
    Ended,
}

/// A player for one attached video or audio file. It belongs to the session
/// rather than to the row that draws it, so scrolling the message away does
/// not stop the music.
pub struct Player {
    stage: Cell<Stage>,
    /// The newest decoded picture of a playing video.
    frame: RefCell<Option<Arc<RenderImage>>>,
    playback: RefCell<Option<Playback>>,
    /// What went wrong, in the words GStreamer or the network used.
    error: RefCell<Option<String>>,
    /// Drawn over the whole window rather than in the row.
    expanded: Cell<bool>,
}

impl Player {
    fn new() -> Self {
        Player {
            stage: Cell::new(Stage::Idle),
            frame: RefCell::new(None),
            playback: RefCell::new(None),
            error: RefCell::new(None),
            expanded: Cell::new(false),
        }
    }

    fn failed(&self, message: &str) {
        tracing::warn!(message, "could not play the attachment");
        self.playback.borrow_mut().take();
        self.stage.set(Stage::Idle);
        *self.error.borrow_mut() = Some(message.to_string());
    }

    /// The picture to draw while this is the expanded one, if it has one.
    pub fn expanded_frame(&self) -> Option<Arc<RenderImage>> {
        self.expanded.get().then(|| self.frame.borrow().clone()).flatten()
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
    }
}

/// "1:05 / 3:20".
fn clock(position: f64, duration: f64) -> String {
    let stamp = |seconds: f64| {
        let seconds = seconds.max(0.0) as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    format!("{} / {}", stamp(position), stamp(duration))
}

impl Ui {
    fn media_player(&self, file_id: &str) -> Rc<Player> {
        self.players
            .borrow_mut()
            .entry(file_id.to_string())
            .or_insert_with(|| Rc::new(Player::new()))
            .clone()
    }

    /// The video being shown over the whole window, if one is.
    pub(super) fn expanded_player(&self) -> Option<(String, Rc<Player>)> {
        self.players
            .borrow()
            .iter()
            .find(|(_, player)| player.expanded.get())
            .map(|(id, player)| (id.clone(), player.clone()))
    }

    /// Play was pressed: resume, start over, or fetch the file and begin.
    fn play_media(self: &Rc<Self>, file: &FileInfo, cx: &mut App) {
        let player = self.media_player(&file.id);
        match player.stage.get() {
            Stage::Loading => {}
            Stage::Playing => {
                if let Some(playback) = player.playback.borrow().as_ref() {
                    playback.set_paused(true);
                }
                player.stage.set(Stage::Paused);
            }
            Stage::Paused => {
                if let Some(playback) = player.playback.borrow().as_ref() {
                    playback.set_paused(false);
                }
                player.stage.set(Stage::Playing);
                self.tick_media(file.id.clone());
            }
            Stage::Ended => {
                if let Some(playback) = player.playback.borrow().as_ref() {
                    playback.restart();
                }
                player.stage.set(Stage::Playing);
                self.tick_media(file.id.clone());
            }
            Stage::Idle => {
                player.stage.set(Stage::Loading);
                player.error.borrow_mut().take();
                let fetch = self.fetch_file(file.id.clone());
                let path = std::env::temp_dir().join(format!(
                    "matterfast-{}.{}",
                    sanitised(&file.id),
                    sanitised(&extension(file))
                ));
                let ui = self.clone();
                let file_id = file.id.clone();
                runtime::spawn(
                    async move {
                        let bytes = fetch.await.map_err(|e| e.to_string())?;
                        tokio::fs::write(&path, bytes)
                            .await
                            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                        Ok::<_, String>(path)
                    },
                    move |result, cx| {
                        let player = ui.media_player(&file_id);
                        match result.and_then(Playback::start) {
                            Ok((playback, events)) => {
                                *player.playback.borrow_mut() = Some(playback);
                                player.stage.set(Stage::Playing);
                                ui.watch_media(file_id.clone(), events);
                                ui.tick_media(file_id);
                            }
                            Err(error) => player.failed(&error),
                        }
                        cx.refresh_windows();
                    },
                );
            }
        }
        cx.refresh_windows();
    }

    /// Follows a playback: its pictures, its end, its failure.
    fn watch_media(self: &Rc<Self>, file_id: String, events: async_channel::Receiver<playback::Event>) {
        let weak = Rc::downgrade(self);
        runtime::receive(events, move |event, cx| {
            let Some(ui) = weak.upgrade() else { return false };
            let player = ui.media_player(&file_id);
            match event {
                playback::Event::Frame(frame) => {
                    if let Some(image) = crate::video::frame_image(frame) {
                        if let Some(old) = player.frame.replace(Some(Arc::new(image))) {
                            cx.drop_image(old, None);
                        }
                    }
                }
                playback::Event::Ended => player.stage.set(Stage::Ended),
                playback::Event::Failed(error) => player.failed(&error),
            }
            cx.refresh_windows();
            player.playback.borrow().is_some()
        });
    }

    /// Redraws now and then while something plays, so its clock moves. Audio
    /// sends no pictures to do it for us.
    fn tick_media(self: &Rc<Self>, file_id: String) {
        let weak = Rc::downgrade(self);
        runtime::after(PROGRESS_TICK, move |cx| {
            let Some(ui) = weak.upgrade() else { return };
            if ui.media_player(&file_id).stage.get() == Stage::Playing {
                cx.refresh_windows();
                ui.tick_media(file_id);
            }
        });
    }

    /// The still for a video, when there is one yet. Asking is what starts it
    /// being made: the head of the file is fetched, and a frame decoded from
    /// it off the main thread.
    fn poster(self: &Rc<Self>, file: &FileInfo) -> Option<Arc<RenderImage>> {
        if let Some(poster) = self.avatars.poster(&file.id) {
            return Some(poster);
        }
        let head = self.avatars.video_head(&file.id)?;
        // A container whose index is not in the head cannot give a picture,
        // and fetching more of a multi-hundred-megabyte file for a thumbnail
        // is not worth it. Tried once either way.
        if !head_playable(&head) || !self.stills_tried.borrow_mut().insert(file.id.clone()) {
            return None;
        }
        let path = std::env::temp_dir().join(format!(
            "matterfast-poster-{}.{}",
            sanitised(&file.id),
            sanitised(&extension(file))
        ));
        let bytes = head.as_ref().clone();
        let ui = self.clone();
        let file_id = file.id.clone();
        runtime::spawn(
            async move {
                tokio::task::spawn_blocking(move || {
                    playback::still(&bytes, &path).and_then(crate::video::frame_image)
                })
                .await
                .ok()
                .flatten()
            },
            move |still, cx| {
                // Kept, so the next draw of that row shows a picture instead
                // of starting a decoder over again.
                if let Some(still) = still {
                    ui.avatars.remember_poster(&file_id, Arc::new(still), cx);
                    cx.refresh_windows();
                }
            },
        );
        None
    }
}

/// Draws the player for one attached file.
pub fn player(ui: &Rc<Ui>, index: usize, file: &FileInfo, cx: &App) -> AnyElement {
    let player = ui.media_player(&file.id);
    let stage = player.stage.get();
    let theme = cx.theme();
    let label = format!("{}  ·  {}", file.name, file.human_size());
    let progress = player
        .playback
        .borrow()
        .as_ref()
        .and_then(|playback| playback.progress())
        .map(|(position, duration)| clock(position, duration));

    // Built where it is used: the same button sits over the poster before
    // playing and in the controls under the picture after.
    let play = || {
        let play_file = file.clone();
        Button::new("play")
            .icon(match stage {
                Stage::Playing => Lucide::Pause,
                Stage::Ended => Lucide::RotateCcw,
                _ => Lucide::Play,
            })
            .small()
            .tooltip(match stage {
                Stage::Playing => "Pause",
                Stage::Ended => "Play again",
                _ => "Play",
            })
            .on_click(ui.click(move |ui, cx| ui.play_media(&play_file, cx)))
    };

    let error = player.error.borrow().clone();
    let note = error.map(|message| {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(message)
    });

    // Audio has nothing to show: it gets controls and no poster.
    if is_audio(file) {
        return v_flex()
            .id(("audio", index))
            .mt_1()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_color(theme.muted_foreground)
                    .child(if stage == Stage::Loading {
                        Spinner::new().small().into_any_element()
                    } else {
                        play().ghost().into_any_element()
                    })
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when_some(progress, |row, progress| {
                        row.child(div().flex_none().text_xs().child(progress))
                    }),
            )
            .when_some(note, |column, note| column.child(note))
            .into_any_element();
    }

    let picture = match stage {
        Stage::Playing | Stage::Paused | Stage::Ended => player.frame.borrow().clone(),
        _ => None,
    }
    .or_else(|| ui.poster(file));

    let mut face = div()
        .relative()
        .w(px(POSTER_WIDTH))
        .max_w_full()
        .h(px(POSTER_HEIGHT))
        .rounded_md()
        .overflow_hidden()
        .bg(gpui_kit::black());
    if let Some(picture) = picture {
        face = face.child(img(picture).size_full().object_fit(ObjectFit::Contain));
    }
    // The button sits over the picture until it plays; after that the
    // controls under it are how it is paused.
    if matches!(stage, Stage::Idle | Stage::Loading | Stage::Ended) {
        face = face.child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(if stage == Stage::Loading {
                    Spinner::new().large().into_any_element()
                } else {
                    play().primary().into_any_element()
                }),
        );
    }

    let mut controls = h_flex()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(theme.muted_foreground);
    if matches!(stage, Stage::Playing | Stage::Paused) {
        let expand_id = file.id.clone();
        controls = controls.child(play().ghost()).child(
            kit::icon_button("expand", Lucide::Maximize, "Fill the window").on_click(ui.click(
                move |ui, cx| {
                    ui.media_player(&expand_id).expanded.set(true);
                    cx.refresh_windows();
                },
            )),
        );
    }
    controls = controls
        .child(div().flex_1().min_w_0().truncate().child(label))
        .when_some(progress, |row, progress| {
            row.child(div().flex_none().child(progress))
        });

    v_flex()
        .id(("video", index))
        .mt_1()
        .gap_1()
        .w(px(POSTER_WIDTH))
        .max_w_full()
        .child(face)
        .child(controls)
        .when_some(note, |column, note| column.child(note))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(mime: &str, extension: &str) -> FileInfo {
        FileInfo {
            mime_type: mime.to_string(),
            extension: extension.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn the_mime_type_decides_when_there_is_one() {
        assert!(is_playable(&file("video/mp4", "mp4")));
        assert!(is_playable(&file("audio/mpeg", "mp3")));
        // An extension we would not have guessed from, and one we would have
        // guessed wrong from.
        assert!(is_playable(&file("video/webm", "")));
        assert!(!is_playable(&file("application/pdf", "pdf")));
        assert!(!is_playable(&file("image/png", "png")));
    }

    #[test]
    fn the_extension_answers_when_the_mime_type_is_empty() {
        assert!(is_playable(&file("", "mp4")));
        assert!(is_playable(&file("", ".MP3")));
        assert!(is_playable(&file("", "opus")));
        assert!(!is_playable(&file("", "pdf")));
    }

    #[test]
    fn nothing_to_go_on_is_not_playable() {
        assert!(!is_playable(&file("", "")));
    }

    #[test]
    fn formats_gstreamer_will_not_have_are_excluded() {
        // Excluded by mime type and by extension alike, because the server
        // may give us either.
        assert!(!is_playable(&file("video/x-ms-wmv", "wmv")));
        assert!(!is_playable(&file("video/mp4", "wmv")));
        assert!(!is_playable(&file("", "avi")));
        assert!(!is_playable(&file("video/mpeg", "")));
    }

    #[test]
    fn audio_is_told_apart_from_video() {
        assert!(is_audio(&file("audio/flac", "flac")));
        assert!(!is_audio(&file("video/mp4", "mp4")));
        assert!(is_audio(&file("", "m4a")));
        assert!(!is_audio(&file("", "mkv")));
    }

    /// A single ISO-BMFF box: 4-byte size (including this header) + 4-byte
    /// type + `payload`.
    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + payload.len());
        out.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    /// `ffmpeg -movflags +faststart` on a real clip lays out `ftyp`, `moov`,
    /// `free`, `mdat`, in that order (verified against `/tmp/mmtest.mp4`) —
    /// `moov` is small and near the front, so a bounded head contains it whole.
    #[test]
    fn a_faststart_head_has_a_full_moov() {
        let mut head = mp4_box(b"ftyp", &[0; 24]);
        head.extend(mp4_box(b"moov", &[0; 200]));
        // The rest of the file (`mdat`) need not even be present.
        assert!(head_playable(&head));
    }

    /// Without `+faststart`, `ffmpeg` instead writes `ftyp`, `free`, `mdat`,
    /// `moov` — `mdat` is the whole video, so a head bounded well short of
    /// the file's end never reaches the trailing `moov`.
    #[test]
    fn a_streaming_unfriendly_head_gives_up() {
        let mut head = mp4_box(b"ftyp", &[0; 24]);
        head.extend(mp4_box(b"free", &[]));
        // Declares a size far bigger than what's actually in `head` — the
        // stand-in for "the rest of a multi-hundred-MB clip".
        head.extend(50u32.to_be_bytes());
        head.extend(b"mdat");
        // No `moov` ever follows within this slice.
        assert!(!head_playable(&head));
    }

    /// `moov`'s header is in the head, but its declared size runs past the
    /// end of what was actually fetched — an incomplete atom is as useless
    /// as no atom, and must not be read out of bounds either.
    #[test]
    fn a_moov_box_cut_off_mid_atom_gives_up() {
        let mut head = mp4_box(b"ftyp", &[0; 24]);
        head.extend(200u32.to_be_bytes());
        head.extend(b"moov");
        head.extend([0; 50]); // far short of the 200 the header promises
        assert!(!head_playable(&head));
    }

    /// WebM/Matroska never needs the box scan at all — only the magic bytes.
    #[test]
    fn a_webm_head_is_always_playable() {
        let mut head = vec![0x1A, 0x45, 0xDF, 0xA3];
        head.extend([0; 16]); // arbitrary EBML content
        assert!(head_playable(&head));
    }

    #[test]
    fn empty_or_garbage_is_not_playable() {
        assert!(!head_playable(&[]));
        assert!(!head_playable(b"not a media file at all"));
    }

    #[test]
    fn the_clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0.0, 200.0), "0:00 / 3:20");
        assert_eq!(clock(65.4, 200.0), "1:05 / 3:20");
        // A position the pipeline reports before it has settled.
        assert_eq!(clock(-1.0, 0.0), "0:00 / 0:00");
    }

    #[test]
    fn a_file_name_part_is_only_letters_and_digits() {
        assert_eq!(sanitised("abc123"), "abc123");
        assert_eq!(sanitised("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitised(""), "bin");
        assert_eq!(sanitised("$(rm -rf)"), "rmrf");
    }
}
