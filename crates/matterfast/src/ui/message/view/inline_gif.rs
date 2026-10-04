/// The largest GIF fetched whole without being asked for.
const INLINE_GIF_BYTES: i64 = 8 * 1024 * 1024;

/// Whether an attachment is an animation small enough to play where it sits.
pub(super) fn plays_inline(file: &mattermost_api::models::FileInfo) -> bool {
    let gif = file.mime_type == "image/gif" || file.extension.eq_ignore_ascii_case("gif");
    gif && file.size > 0 && file.size <= INLINE_GIF_BYTES
}
