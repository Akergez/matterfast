use rusqlite::params;

use super::error::Error;
use super::handle::Store;

impl Store {
    /// Drops every post in a channel past the newest `keep_per_channel`, in
    /// every channel at once.
    ///
    /// Channels, members and users are left alone: there is one row per thing
    /// that exists on the server, so they are bounded by the account, not by
    /// how much it is used. Posts are the only table that grows with time.
    pub async fn prune(&self, keep_per_channel: usize) -> Result<(), Error> {
        let conn = self.conn.lock().await;
        // One statement: number the posts of each channel newest-first and
        // delete everything numbered past the limit. The tiebreak matches
        // `posts()` exactly, so pruning can never drop a post that a read of
        // the same size would have returned.
        let n = conn.execute(
            "DELETE FROM posts WHERE id IN (
                 SELECT id FROM (
                     SELECT id, ROW_NUMBER() OVER (
                         PARTITION BY channel_id ORDER BY create_at DESC, id DESC
                     ) AS rank FROM posts
                 ) WHERE rank > ?1
             )",
            params![keep_per_channel as i64],
        )?;
        tracing::debug!(pruned = n, keep_per_channel, "trimmed the post cache");

        // A delete never shrinks the file, it only puts pages on the free list,
        // and the next posts to arrive reuse them — so in the steady state, a
        // pass that trims a few percent off each channel wants no VACUUM at
        // all, and running one would rewrite the whole database for nothing.
        //
        // Measured, 20 channels x 1000 posts, ~21 MB: trimming to 100 per
        // channel freed 4622 of 5183 pages and left the file at 21 MB; VACUUM
        // brought it to 2.1 MB; refilling it grew back to 21 MB and no further.
        // So it is worth exactly one case — the first prune on a file that grew
        // before there was a cap — and that case announces itself on the free
        // list. Vacuum when most of the file is holes, not on a schedule.
        let free: i64 = conn.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        let total: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        if free * 2 > total {
            tracing::info!(free, total, "post cache is mostly free space, vacuuming");
            conn.execute_batch("VACUUM")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use mattermost_api::models::Post;

    use super::super::constants::DEFAULT_KEEP_PER_CHANNEL;
    use super::super::paths::db_path;
    use super::super::test_support::{post, temp_dir};
    use super::*;

    #[tokio::test]
    async fn pruning_keeps_the_newest_few_of_every_channel() {
        // The whole statement rests on window functions; SQLite has had them
        // since 3.25 (2018) and this is the bundled build, so check the claim
        // rather than assume it.
        assert!(
            rusqlite::version_number() >= 3_025_000,
            "bundled SQLite {} is too old for ROW_NUMBER() OVER (...)",
            rusqlite::version()
        );

        let dir = temp_dir("prune");
        let store = Store::open_at(&dir, "example.com").unwrap();
        let mut posts: Vec<Post> = (0..50).map(|i| post(&format!("a{i}"), "c1", i)).collect();
        posts.extend((0..3).map(|i| post(&format!("b{i}"), "c2", i)));
        store.save_posts(posts).await.unwrap();

        store.prune(10).await.unwrap();

        // Exactly the newest ten of the busy channel...
        let kept = store.posts("c1", 1000).await.unwrap();
        assert_eq!(kept.len(), 10);
        assert_eq!(kept.first().unwrap().id, "a40");
        assert_eq!(kept.last().unwrap().id, "a49");
        // ...and the quiet channel is untouched: the limit is per channel.
        assert_eq!(store.posts("c2", 1000).await.unwrap().len(), 3);

        // Idempotent: a second pass has nothing left to take.
        store.prune(10).await.unwrap();
        assert_eq!(store.posts("c1", 1000).await.unwrap().len(), 10);
        assert_eq!(store.posts("c2", 1000).await.unwrap().len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pruning_an_empty_store_is_not_an_error() {
        let dir = temp_dir("prune-empty");
        let store = Store::open_at(&dir, "example.com").unwrap();
        store.prune(DEFAULT_KEEP_PER_CHANNEL).await.unwrap();
        // And zero is a real answer, not a no-op that quietly keeps everything.
        store.save_posts(vec![post("p1", "c1", 100)]).await.unwrap();
        store.prune(0).await.unwrap();
        assert!(store.posts("c1", 10).await.unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    /// The one case VACUUM is there for: a file that grew before there was a
    /// cap, trimmed hard the first time a pruning build runs.
    #[tokio::test]
    async fn a_big_trim_gives_the_disk_space_back() {
        let dir = temp_dir("prune-size");
        let store = Store::open_at(&dir, "example.com").unwrap();
        let posts: Vec<Post> = (0..2000)
            .map(|i| {
                let mut p = post(&format!("p{i}"), "c1", i);
                p.message = "x".repeat(600);
                p
            })
            .collect();
        store.save_posts(posts).await.unwrap();
        let path = db_path(&dir, "example.com");
        let size = |p: &PathBuf| fs::metadata(p).unwrap().len();
        let before = size(&path);

        store.prune(10).await.unwrap();
        assert!(
            size(&path) < before / 2,
            "pruned 99% of the rows and the file is still {} of {before} bytes",
            size(&path)
        );

        // ...and the steady state does not: a pass that takes nothing leaves
        // the file alone rather than rewriting it every time.
        let settled = size(&path);
        store.prune(10).await.unwrap();
        assert_eq!(size(&path), settled);

        let _ = fs::remove_dir_all(&dir);
    }
}
