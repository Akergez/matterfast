//! Times, the way a person reads them.
//!
//! Mattermost timestamps are Unix **milliseconds**, everywhere except the few
//! places noted below, and everything shown to a person is in their own zone.

mod expiry_phrase;
mod format_day;
mod format_relative;
mod format_time;
mod has_expired;
mod local;
mod now_ms;
mod parse_expiry;
mod rfc3339_local;
#[cfg(test)]
mod test_support;
mod unique;

pub use expiry_phrase::expiry_phrase;
pub use format_day::format_day;
pub use format_relative::format_relative;
pub use format_time::format_time;
pub use has_expired::has_expired;
pub use now_ms::now_ms;
pub use rfc3339_local::rfc3339_local;
pub use unique::unique;
