/// How wide a column is: a fraction of the window, within bounds.
pub(super) fn column_width(window: f32, fraction: f32, min: f32, max: f32) -> f32 {
    (window * fraction).clamp(min, max)
}

/// How wide a side column is: what the person dragged it to if they did, its
/// share of the window otherwise. A dragged width may go past the share's own
/// bounds, but never so far that the conversation is squeezed out — `most` is
/// what is left for the column in this window, and wins over `least`.
pub(super) fn dragged_width(dragged: Option<f32>, share: f32, least: f32, most: f32) -> f32 {
    dragged.map_or(share, |width| width.max(least)).min(most)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_column_keeps_its_width_and_leaves_room_for_the_conversation() {
        // Untouched, it is its share of the window.
        assert_eq!(dragged_width(None, 316.8, 180.0, 594.0), 316.8);
        // Dragged, it is what it was dragged to, past the share's own bounds.
        assert_eq!(dragged_width(Some(500.0), 316.8, 180.0, 594.0), 500.0);
        assert_eq!(dragged_width(Some(40.0), 316.8, 180.0, 594.0), 180.0);
        // A width chosen in a wide window does not swallow a narrow one.
        assert_eq!(dragged_width(Some(500.0), 220.0, 180.0, 360.0), 360.0);
    }

    #[test]
    fn a_column_is_a_fraction_of_the_window_within_bounds() {
        // The sidebar: a quarter of the window, never thinner than a channel
        // name nor wider than one needs.
        assert_eq!(column_width(1320.0, 0.24, 220.0, 360.0), 316.8);
        assert_eq!(column_width(700.0, 0.24, 220.0, 360.0), 220.0);
        assert_eq!(column_width(2560.0, 0.24, 220.0, 360.0), 360.0);
    }
}
