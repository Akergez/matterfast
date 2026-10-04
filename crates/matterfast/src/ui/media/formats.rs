use mattermost_api::models::FileInfo;

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

pub(super) fn extension(file: &FileInfo) -> String {
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
