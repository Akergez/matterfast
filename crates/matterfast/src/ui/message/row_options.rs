use mattermost_api::models::Millis;

/// Messages from the same author within this window are drawn as one group,
/// without repeating the avatar and name.
pub const GROUPING_WINDOW_MS: Millis = 5 * 60 * 1000;

#[derive(Clone, Copy)]
pub struct RowOptions {
    /// Continuation of the previous author's group: no avatar, no name.
    pub grouped: bool,
    /// Offer the "N replies" footer. False inside a thread.
    pub show_thread_footer: bool,
    /// Briefly marked out, because something just navigated to it.
    pub highlight: bool,
}
