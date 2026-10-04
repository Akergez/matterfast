/// "1:05 / 3:20".
pub(super) fn clock(position: f64, duration: f64) -> String {
    let stamp = |seconds: f64| {
        let seconds = seconds.max(0.0) as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    format!("{} / {}", stamp(position), stamp(duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0.0, 200.0), "0:00 / 3:20");
        assert_eq!(clock(65.4, 200.0), "1:05 / 3:20");
        // A position the pipeline reports before it has settled.
        assert_eq!(clock(-1.0, 0.0), "0:00 / 0:00");
    }
}
