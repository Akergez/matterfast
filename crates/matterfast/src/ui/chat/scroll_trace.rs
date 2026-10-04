/// Whether scroll diagnostics are on: `MATTERFAST_SCROLL_TRACE=1`. Read once,
/// because it is asked on every scroll tick.
pub(crate) fn scroll_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("MATTERFAST_SCROLL_TRACE")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    })
}
