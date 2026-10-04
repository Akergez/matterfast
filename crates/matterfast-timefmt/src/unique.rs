use std::sync::atomic::{AtomicI64, Ordering};

use crate::now_ms::now_ms;

/// A number no earlier call returned. Used to name things that only have to
/// be distinct for the life of the process: a message not yet confirmed by
/// the server, a temporary file.
pub fn unique() -> i64 {
    static LAST: AtomicI64 = AtomicI64::new(0);
    let now = now_ms();
    // The clock is the starting point, so two runs do not collide either; the
    // counter is what makes two calls in one millisecond differ.
    LAST.fetch_max(now, Ordering::Relaxed);
    LAST.fetch_add(1, Ordering::Relaxed) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_numbers_are() {
        let seen: std::collections::HashSet<i64> = (0..1000).map(|_| unique()).collect();
        assert_eq!(seen.len(), 1000);
    }
}
