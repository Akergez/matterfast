use chrono::Local;
use mattermost_api::models::Millis;

use crate::local::local;

/// Relative time for list views ("2m", "3h", "Tue").
pub fn format_relative(millis: Millis) -> String {
    let Some(then) = local(millis) else {
        return String::new();
    };
    let seconds = (Local::now() - then).num_seconds();
    match seconds {
        s if s < 60 => "now".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s if s < 7 * 86_400 => then.format("%a").to_string(),
        _ => then.format("%-d %b").to_string(),
    }
}
