use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::app::App;
use crate::handlers::{
    add_reaction, categories, channel_posts, client_config, create_post, emoji_by_name,
    emoji_image, emoji_list, emoji_search, empty_array, empty_object, get_post, group_members,
    groups,
    login, me, my_channel_members, my_channels, my_team_members, my_teams, post_action,
    post_thread, remove_reaction, search_posts, statuses, team_unreads, threads, unread_posts,
    user_image, users_autocomplete, users_by_ids, users_by_usernames, users_query, users_search,
    view_channel, websocket,
};

/// The Mattermost REST API, as mounted under `/api/v4`.
pub(super) fn api() -> Router<Arc<App>> {
    Router::new()
        .route("/users/login", post(login))
        .route("/users/logout", post(empty_object))
        .route("/config/client", get(client_config))
        .route("/license/client", get(empty_object))
        .route("/users/me", get(me))
        .route("/users/me/preferences", get(empty_array))
        .route("/users/me/teams", get(my_teams))
        .route("/users/me/teams/members", get(my_team_members))
        .route("/users/me/teams/unread", get(team_unreads))
        .route("/users/me/teams/{team}/channels", get(my_channels))
        .route(
            "/users/me/teams/{team}/channels/members",
            get(my_channel_members),
        )
        .route(
            "/users/me/teams/{team}/channels/categories",
            get(categories),
        )
        .route("/users/me/teams/{team}/threads", get(threads))
        .route("/users/{user}/teams/{team}/threads", get(threads))
        .route("/users/ids", post(users_by_ids))
        .route("/users/search", post(users_search))
        .route("/users/autocomplete", get(users_autocomplete))
        .route("/users/usernames", post(users_by_usernames))
        .route("/users", get(users_query))
        .route("/users/status/ids", post(statuses))
        .route("/users/{user}/image", get(user_image))
        .route(
            "/users/me/channels/{channel}/posts/unread",
            get(unread_posts),
        )
        .route("/channels/{channel}/posts", get(channel_posts))
        .route("/channels/members/me/view", post(view_channel))
        .route("/posts", post(create_post))
        .route("/posts/{post}", get(get_post))
        .route("/posts/{post}/thread", get(post_thread))
        .route("/posts/{post}/actions/{action}", post(post_action))
        .route("/reactions", post(add_reaction))
        .route(
            "/users/{user}/posts/{post}/reactions/{emoji}",
            delete(remove_reaction),
        )
        .route("/teams/{team}/posts/search", post(search_posts))
        .route("/emoji", get(emoji_list))
        .route("/groups", get(groups))
        .route("/groups/{id}/members", get(group_members))
        .route("/emoji/search", post(emoji_search))
        .route("/emoji/name/{name}", get(emoji_by_name))
        .route("/emoji/{id}/image", get(emoji_image))
        .route("/websocket", get(websocket))
}
