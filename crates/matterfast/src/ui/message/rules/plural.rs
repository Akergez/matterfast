/// `one` for exactly one, `many` otherwise.
pub fn plural(n: i64, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}
