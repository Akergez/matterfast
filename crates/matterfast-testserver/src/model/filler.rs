use crate::ids::id;

/// Extra open channels beyond the three hand-written ones. Three rows rebuild
/// too fast to time, so any work on how the sidebar redraws itself needs a
/// realistic list to show up at all — `MM_CHANNELS=100` is what the sidebar
/// profiling runs use.
pub(crate) fn filler_count() -> usize {
    std::env::var("MM_CHANNELS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

pub(crate) fn filler_id(n: usize) -> String {
    id("cx", n as i64)
}

/// The inverse of `filler_id`, so the per-channel handlers can answer for a
/// generated channel without keeping a table of them.
pub(crate) fn filler_index(channel_id: &str) -> Option<usize> {
    channel_id.strip_prefix("cx")?.parse().ok()
}
