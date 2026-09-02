//! Playing an attached video or audio file where it was posted.
//!
//! Nothing here downloads on its own. A [`Player`] starts as a poster — the
//! name, the size and a play button — and only asks for the bytes when someone
//! presses play, because a channel scrolled past ten videos would otherwise
//! fetch ten videos.
//!
//! The file itself is behind the session token, so a URL handed to GStreamer
//! would come back 401; the bytes arrive the same way `open_image` gets an
//! image, through the client, and are handed back with [`Player::set_data`].

use std::cell::RefCell;
use std::path::PathBuf;

use gtk::prelude::*;
use mattermost_api::models::FileInfo;

/// Tall enough to watch, short enough that a video does not push the
/// conversation off the screen — the same 180px an `.attachment-image` gets.
const MAX_HEIGHT: i32 = 180;

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
fn is_audio(file: &FileInfo) -> bool {
    let mime = file.mime_type.trim().to_ascii_lowercase();
    if !mime.is_empty() {
        return mime.starts_with("audio/");
    }
    AUDIO_EXT.contains(&extension(file).as_str())
}

/// A player for one attached video or audio file.
///
/// Drop it to clean up the temporary file the bytes were written to; the
/// widget stops playing on its own when it leaves the window.
/// The still shown before a video is played. Matches the size an image
/// attachment is drawn at, so a channel of clips and photos reads evenly.
const POSTER_WIDTH: i32 = 420;
const POSTER_HEIGHT: i32 = 260;

pub struct Player {
    pub widget: gtk::Widget,
    body: gtk::Box,
    poster: gtk::Widget,
    play: gtk::Button,
    spinner: gtk::Spinner,
    audio: bool,
    id: String,
    title: String,
    extension: String,
    temp: RefCell<Option<PathBuf>>,
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(path) = self.temp.borrow_mut().take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Player {
    /// `on_load` is called with the file id when the user asks to play;
    /// answer it with [`Player::set_data`].
    /// `poster_image` is the server's own thumbnail, when it has made one —
    /// it does for video, and a still of the clip says far more about whether
    /// to press play than a filename does.
    pub fn new(
        file: &FileInfo,
        poster_image: Option<gtk::gdk::Texture>,
        on_load: impl Fn(String) + 'static,
    ) -> Player {
        let play = gtk::Button::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Play")
            .valign(gtk::Align::Center)
            .build();
        play.add_css_class("flat");
        play.add_css_class("circular");

        // Takes the play button's place for as long as the download runs.
        let spinner = gtk::Spinner::builder()
            .valign(gtk::Align::Center)
            .visible(false)
            .build();

        let label = gtk::Label::builder()
            .label(format!("{}  ·  {}", file.name, file.human_size()))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        controls.add_css_class("dim-label");
        controls.append(&play);
        controls.append(&spinner);
        controls.append(&label);

        // With a thumbnail the controls sit *over* the still, the way a video
        // reads everywhere else. Without one, a video still has to read as a
        // video rather than as a line of text: a tile with the play button in
        // the middle of it, the name underneath.
        let poster: gtk::Widget = match &poster_image {
            Some(texture) => {
                // Height only, and the width capped by a clamp: a width
                // request is a floor, and in a narrow panel GTK cannot honour
                // it.
                let still = gtk::Picture::builder()
                    .paintable(texture)
                    .content_fit(gtk::ContentFit::Contain)
                    .halign(gtk::Align::Start)
                    .height_request(POSTER_HEIGHT)
                    .build();
                still.add_css_class("attachment-image");

                play.set_halign(gtk::Align::Center);
                play.set_valign(gtk::Align::Center);
                play.add_css_class("osd");
                play.add_css_class("circular");
                spinner.set_halign(gtk::Align::Center);
                spinner.set_valign(gtk::Align::Center);
                label.set_valign(gtk::Align::End);
                label.set_halign(gtk::Align::Start);
                label.add_css_class("osd");

                let sized = adw::Clamp::builder()
                    .maximum_size(POSTER_WIDTH)
                    .halign(gtk::Align::Start)
                    .child(&still)
                    .build();
                let overlay = gtk::Overlay::builder().child(&sized).build();
                overlay.add_overlay(&play);
                overlay.add_overlay(&spinner);
                overlay.add_overlay(&label);
                overlay.upcast()
            }
            None if is_audio(file) => controls.upcast(),
            // No still to show, so the tile is a blank one. The play button
            // is the whole of it, sized like a video rather than like a row,
            // because that shape is what says "this is something to watch".
            None => {
                play.set_halign(gtk::Align::Center);
                play.set_valign(gtk::Align::Center);
                play.add_css_class("osd");
                spinner.set_halign(gtk::Align::Center);
                spinner.set_valign(gtk::Align::Center);

                // Out of the row before into the tile: a widget cannot be
                // given a second parent while it still has the first.
                controls.remove(&play);
                controls.remove(&spinner);
                controls.set_halign(gtk::Align::Start);

                let face = gtk::Overlay::builder()
                    .height_request(POSTER_HEIGHT)
                    .build();
                face.add_css_class("video-tile");
                face.add_overlay(&play);
                face.add_overlay(&spinner);

                let tile = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(4)
                    .halign(gtk::Align::Start)
                    .build();
                tile.append(
                    &adw::Clamp::builder()
                        .maximum_size(POSTER_WIDTH)
                        .child(&face)
                        .build(),
                );
                tile.append(&controls);
                tile.upcast()
            }
        };

        play.connect_clicked({
            let spinner = spinner.clone();
            let id = file.id.clone();
            move |play| {
                play.set_visible(false);
                spinner.set_visible(true);
                spinner.start();
                on_load(id.clone());
            }
        });

        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        body.append(&poster);

        Player {
            widget: body.clone().upcast(),
            body,
            poster: poster.upcast(),
            play,
            spinner,
            audio: is_audio(file),
            id: file.id.clone(),
            title: file.name.clone(),
            extension: extension(file),
            temp: RefCell::new(None),
        }
    }

    /// Hands over the downloaded bytes; the player starts from them.
    pub fn set_data(&self, bytes: Vec<u8>) {
        self.spinner.stop();
        self.spinner.set_visible(false);
        if self.temp.borrow().is_some() {
            return;
        }

        // A temporary file rather than a `gio::MemoryInputStream`, because
        // GTK's GStreamer backend only learned to read a `MediaFile` built
        // from a stream recently: before that it answered one with "Input
        // Streams are currently not supported. Please pass a File based
        // MediaFile." It works here on GTK 4.22 and does nothing at all on the
        // 4.18 in the GNOME 48 flatpak runtime; `for_filename` works on both.
        let path = std::env::temp_dir().join(format!(
            "mm-adw-{}.{}",
            sanitised(&self.id),
            sanitised(&self.extension)
        ));
        if let Err(e) = std::fs::write(&path, &bytes) {
            self.failed(&format!("could not write {}: {e}", path.display()));
            return;
        }
        *self.temp.borrow_mut() = Some(path.clone());

        let media = gtk::MediaFile::for_filename(&path);
        media.set_loop(false);
        media.connect_error_notify({
            let body = self.body.clone();
            move |media| {
                let Some(error) = media.error() else { return };
                tracing::warn!(error = %error, "could not play the attachment");
                body.append(&note(&error.to_string()));
            }
        });

        let player: gtk::Widget = if self.audio {
            // Audio has no picture to show, so the controls are the widget.
            gtk::MediaControls::new(Some(&media)).upcast()
        } else {
            let video = gtk::Video::for_media_stream(Some(&media));
            // We start it ourselves below, on the click that asked for it.
            video.set_autoplay(false);
            no_offload(&video);
            video.set_height_request(MAX_HEIGHT);
            // A `GtkVideo` asks for the full size of whatever it is showing,
            // and a 1080p attachment would take the conversation with it. The
            // clamp is what actually holds the height down; the picture inside
            // scales to fit rather than being cropped.
            let clamp = adw::Clamp::builder()
                .maximum_size(MAX_HEIGHT)
                .halign(gtk::Align::Start)
                .child(&video)
                .build();
            clamp.set_orientation(gtk::Orientation::Vertical);

            // 180px in a column of messages is a postage stamp. The same
            // stream over the whole window is what the web calls the lightbox,
            // and it is the same object, so pausing there pauses here.
            let expand = gtk::Button::builder()
                .icon_name("view-fullscreen-symbolic")
                .tooltip_text("Fill the window")
                .halign(gtk::Align::End)
                .valign(gtk::Align::Start)
                .margin_top(6)
                .margin_end(6)
                .build();
            expand.add_css_class("osd");
            expand.add_css_class("circular");
            expand.connect_clicked({
                let media = media.clone();
                let title = self.title.clone();
                move |button| {
                    let Some(window) = button.root().and_downcast::<adw::ApplicationWindow>()
                    else {
                        return;
                    };
                    super::lightbox::show_media(&window, &title, media.upcast_ref());
                }
            });

            let framed = gtk::Overlay::builder().child(&clamp).build();
            framed.add_overlay(&expand);
            framed.upcast()
        };

        self.body.remove(&self.poster);
        self.body.append(&player);
        media.play();
    }

    /// Puts the play button back and says why nothing happened.
    fn failed(&self, message: &str) {
        tracing::warn!(message, "could not play the attachment");
        self.play.set_visible(true);
        self.body.append(&note(message));
    }
}

/// Keeps the video frames inside this process instead of handing them to the
/// compositor.
///
/// GTK's graphics offload passes the decoder's dmabuf straight through to a
/// Wayland subsurface, so the buffer is imported by the compositor — and when
/// that import goes wrong on a driver, what dies is the whole session rather
/// than this window. That is what happened here on an Intel Arrow Lake i915;
/// the cause is unproven, but with offload off the same fault can only take
/// down the app.
///
/// ponytail: turned off everywhere rather than only where it misbehaves. The
/// cost is a copy per frame for a video in a chat window. Revisit when the
/// driver is known good.
fn no_offload(video: &gtk::Video) {
    video.set_graphics_offload(gtk::GraphicsOffloadEnabled::Disabled);
}

fn note(message: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(message)
        .xalign(0.0)
        .wrap(true)
        .build();
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label
}

/// Both halves of the temporary file's name come from the server, and both end
/// up in a path.
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
}
