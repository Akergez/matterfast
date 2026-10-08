//! Client-side state.
//!
//! This is the in-memory shape mattermost-mobile persists to SQLite. Two splits
//! from that design are worth keeping even before there is a database:
//!
//! * the **channel** and **my membership** live in separate maps, because the
//!   membership is rewritten on nearly every websocket event while the channel
//!   object almost never changes;
//! * **posts are stored per channel in a single ordered block**. Real clients
//!   keep several blocks with gaps between them (`postsInChannel` in the
//!   webapp, `PostsInChannel` in mobile) — see the note on [`ChannelFeed`].

mod active_call;
mod app_state;
mod channel_feed;
mod names;
mod new;
mod posts;
mod presence;
mod reactions;
mod reply_ledger;
mod shared_state;
mod chat_list;
mod sidebar_groups;
mod typing;
mod unread;

pub use active_call::ActiveCall;
pub use app_state::AppState;
pub use channel_feed::ChannelFeed;
pub use shared_state::SharedState;
