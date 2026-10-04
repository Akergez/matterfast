use chrono::Local;

use crate::parse_expiry::parse_expiry;

/// Whether a custom status has run out. Expired statuses stay on the user
/// object until the server clears them, so an old "on holiday" would otherwise
/// sit next to someone's name for weeks after they came back.
pub fn has_expired(expires_at: &str) -> bool {
    parse_expiry(expires_at).is_some_and(|when| when < Local::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::in_hours;

    #[test]
    fn an_expired_status_is_recognised() {
        assert!(has_expired(&in_hours(-1)));
        assert!(!has_expired(&in_hours(1)));
        assert!(!has_expired(""));
    }
}
