use super::post::Post;

#[derive(Default)]
pub(crate) struct Db {
    pub(crate) posts: Vec<Post>,
    pub(crate) next_id: i64,
}
