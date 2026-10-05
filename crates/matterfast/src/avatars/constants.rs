/// Marks a cache key as a file thumbnail rather than a user's picture.
pub(super) const FILE_PREFIX: &str = "file:";
/// Marks a cache key as a file's larger preview image, on the same cache but
/// never colliding with its own thumbnail entry.
pub(super) const FILE_PREVIEW_PREFIX: &str = "preview:";
/// Marks a cache key as an uploaded file fetched whole, because a preview of
/// an animation is a still.
pub(super) const ANIMATION_PREFIX: &str = "animation:";
/// Marks a cache key as a custom emoji, looked up by name rather than by id.
pub(super) const EMOJI_PREFIX: &str = "emoji:";
/// Marks a cache key as a video's head bytes (see `Avatars::video_head`).
pub(super) const VIDEO_HEAD_PREFIX: &str = "video-head:";
/// Marks a cache key as a still taken from a video, which is made here rather
/// than fetched.
pub(super) const POSTER_PREFIX: &str = "poster:";

/// How much of a video file to fetch for a poster attempt: enough to hold
/// `ftyp` + `moov` for a phone-shot clip's sample table, tiny next to the
/// clip itself. A file whose `moov` sits after `mdat` (no `-movflags
/// +faststart`) will not fit — `ui::media::head_playable` detects that
/// cheaply from what did come back, and the caller gives up rather than
/// asking for more.
pub(super) const VIDEO_HEAD_BYTES: u64 = 1_500_000;

/// How much decoded picture to hold, in bytes. Counting entries was the wrong
/// unit by two orders of magnitude: an avatar is a few kilobytes and a posted
/// screenshot at draw size is four megabytes, so "a hundred and fifty of
/// them" meant anywhere between half a megabyte and six hundred. Evicting one
/// costs a refetch — and a redraw of everything when it lands, which is why
/// the one to go is the one longest unseen (`Inner::seen`) and not the one
/// that has been here longest.
pub(super) const MAX_TEXTURE_BYTES: usize = 96 * 1024 * 1024;

/// Video heads are a megabyte and a half each and are only read once, to make
/// a poster out of. A handful is plenty.
pub(super) const MAX_HEADS: usize = 4;

/// The widest a picture is ever drawn: `message::scaled_size`'s cap, doubled
/// for a HiDPI screen. Decoding a 4000px photo to keep 500 of them is how a
/// conversation full of screenshots turns into gigabytes.
pub(super) const MAX_DECODED: u32 = 1000;

/// How much decoded animation one picture may hold. Past this it is shown
/// as a still: a long clip saved as a GIF is hundreds of megabytes of frames.
pub(super) const MAX_ANIMATION_BYTES: usize = 48 * 1024 * 1024;
