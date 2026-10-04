/// Bumped whenever the tables or the shape of a `body` blob change.
pub(super) const SCHEMA_VERSION: i64 = 1;

/// Each row keeps its model verbatim as JSON in `body`, with real columns only
/// for what is queried on. The models carry dozens of fields, half of them
/// free-form bags (`props`, `notify_props`, `metadata`); normalising them would
/// be a schema to maintain against a server that adds fields between releases,
/// while serde already round-trips them exactly.
pub(super) const SCHEMA: &str = "
    CREATE TABLE meta (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
    CREATE TABLE channels (id TEXT PRIMARY KEY, body TEXT NOT NULL);
    CREATE TABLE channel_members (
        channel_id TEXT NOT NULL,
        user_id    TEXT NOT NULL,
        body       TEXT NOT NULL,
        PRIMARY KEY (channel_id, user_id)
    );
    CREATE TABLE users (id TEXT PRIMARY KEY, body TEXT NOT NULL);
    CREATE TABLE posts (
        id         TEXT PRIMARY KEY,
        channel_id TEXT NOT NULL,
        create_at  INTEGER NOT NULL,
        body       TEXT NOT NULL
    );
    -- The only query shape there is: the newest N posts of one channel.
    CREATE INDEX posts_by_channel ON posts (channel_id, create_at);
";

/// How many posts per channel [`Store::prune`](super::Store::prune) keeps by
/// default.
///
/// Generous on purpose — the store exists so that history is *there*, and a
/// number that only covers the first screen would make it a splash screen. The
/// window draws 60 posts on a cold start (`ui::INITIAL_POSTS`), so this is
/// roughly sixteen screens of scrollback that works with the network down.
///
/// Bounded on purpose too: a post's JSON is on the order of a kilobyte once
/// metadata and reactions are on it, which puts a busy channel near a megabyte
/// and the whole file in the tens of megabytes for an account with dozens of
/// them. That is a cache size worth having; a year of scrollback is not.
pub const DEFAULT_KEEP_PER_CHANNEL: usize = 1_000;

pub(super) const DROP_ALL: &str = "
    DROP TABLE IF EXISTS meta;
    DROP TABLE IF EXISTS channels;
    DROP TABLE IF EXISTS channel_members;
    DROP TABLE IF EXISTS users;
    DROP TABLE IF EXISTS posts;
";
