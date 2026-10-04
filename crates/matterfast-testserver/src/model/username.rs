use crate::constants::{LENA, MIKK, SARA};

pub(crate) fn username(user_id: &str) -> &'static str {
    match user_id {
        LENA => "lena",
        MIKK => "mikk",
        SARA => "sara",
        _ => "anton",
    }
}
