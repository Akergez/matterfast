use super::post::Post;

#[derive(Default)]
pub(crate) struct Db {
    pub(crate) posts: Vec<Post>,
    pub(crate) next_id: i64,
    /// The account's preferences, as saved: one per category and name.
    pub(crate) preferences: Vec<serde_json::Value>,
}
