//! `POST /teams/{team}/posts/search`: the grammar of a search, and answering it.

mod day_number;
mod matches;
mod parse_search;
mod search_posts;
mod search_terms;

pub(crate) use search_posts::search_posts;
