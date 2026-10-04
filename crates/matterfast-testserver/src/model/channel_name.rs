use super::filler::filler_index;
use crate::constants::{DEV, DM_LENA};

pub(crate) fn channel_name(channel_id: &str) -> String {
    match channel_id {
        DEV => "Development".to_string(),
        DM_LENA => String::new(),
        _ => match filler_index(channel_id) {
            Some(n) => format!("Channel {n:02}"),
            None => "General".to_string(),
        },
    }
}
