//! One function per endpoint the client calls, grouped by what they serve.

mod categories;
mod client_config;
mod emoji;
mod empty;
mod groups;
mod login;
mod me;
mod my_channel_members;
mod my_channels;
mod my_team_members;
mod my_teams;
mod posts;
mod reactions;
mod search;
mod team_unreads;
mod threads;
mod users;
mod view_channel;
mod websocket;
mod zed;

pub(crate) use categories::categories;
pub(crate) use client_config::client_config;
pub(crate) use emoji::{emoji_by_name, emoji_image, emoji_list, emoji_search};
pub(crate) use empty::{empty_array, empty_object};
pub(crate) use groups::{group_members, groups};
pub(crate) use login::login;
pub(crate) use me::me;
pub(crate) use my_channel_members::my_channel_members;
pub(crate) use my_channels::my_channels;
pub(crate) use my_team_members::my_team_members;
pub(crate) use my_teams::my_teams;
pub(crate) use posts::{
    channel_posts, create_post, get_post, post_action, post_thread, unread_posts,
};
pub(crate) use reactions::{add_reaction, remove_reaction};
pub(crate) use search::search_posts;
pub(crate) use team_unreads::team_unreads;
pub(crate) use threads::threads;
pub(crate) use users::{
    statuses, user_image, users_autocomplete, users_by_ids, users_by_usernames, users_query,
    users_search,
};
pub(crate) use view_channel::view_channel;
pub(crate) use websocket::websocket;
pub(crate) use zed::{zed_extension_download, zed_extensions};
