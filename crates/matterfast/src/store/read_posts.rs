use mattermost_api::models::Post;
use rusqlite::{params, OptionalExtension};

use super::error::Error;
use super::handle::Store;

impl Store {
    /// Newest `limit` posts in a channel, returned oldest first — the order a
    /// feed is drawn in.
    pub async fn posts(&self, channel_id: &str, limit: usize) -> Result<Vec<Post>, Error> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare_cached(
            "SELECT body FROM posts WHERE channel_id = ?1
             ORDER BY create_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![channel_id, limit as i64], |r| r.get::<_, String>(0))?;
        let mut posts = Vec::new();
        for row in rows {
            posts.push(serde_json::from_str(&row?)?);
        }
        posts.reverse();
        Ok(posts)
    }

    /// A cached window around one stable post, returned oldest first. The
    /// anchor itself is included exactly once; ties use the post id just like
    /// `posts()` so two messages sharing a millisecond remain deterministic.
    pub async fn posts_around(
        &self,
        channel_id: &str,
        post_id: &str,
        before: usize,
        after: usize,
    ) -> Result<Vec<Post>, Error> {
        let conn = self.conn.lock().await;
        let anchor: Option<(i64, String)> = conn
            .query_row(
                "SELECT create_at, body FROM posts WHERE channel_id = ?1 AND id = ?2",
                params![channel_id, post_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((create_at, body)) = anchor else {
            return Ok(Vec::new());
        };

        let mut older = Vec::new();
        let mut stmt = conn.prepare_cached(
            "SELECT body FROM posts WHERE channel_id = ?1
             AND (create_at < ?2 OR (create_at = ?2 AND id < ?3))
             ORDER BY create_at DESC, id DESC LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![channel_id, create_at, post_id, before as i64],
            |row| row.get::<_, String>(0),
        )?;
        for row in rows {
            older.push(serde_json::from_str(&row?)?);
        }
        older.reverse();
        older.push(serde_json::from_str(&body)?);

        let mut stmt = conn.prepare_cached(
            "SELECT body FROM posts WHERE channel_id = ?1
             AND (create_at > ?2 OR (create_at = ?2 AND id > ?3))
             ORDER BY create_at ASC, id ASC LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![channel_id, create_at, post_id, after as i64],
            |row| row.get::<_, String>(0),
        )?;
        for row in rows {
            older.push(serde_json::from_str(&row?)?);
        }
        Ok(older)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::test_support::{post, temp_dir};
    use super::*;

    #[tokio::test]
    async fn posts_are_the_newest_few_but_read_oldest_first() {
        let dir = temp_dir("order");
        let store = Store::open_at(&dir, "example.com").unwrap();
        let mut posts: Vec<Post> = (0..50).map(|i| post(&format!("p{i}"), "c1", i)).collect();
        posts.push(post("other", "c2", 999));
        store.save_posts(posts).await.unwrap();

        let got = store.posts("c1", 10).await.unwrap();
        assert_eq!(got.len(), 10);
        // The newest ten...
        assert_eq!(got.first().unwrap().id, "p40");
        assert_eq!(got.last().unwrap().id, "p49");
        // ...ascending, and nothing from the other channel.
        assert!(got.windows(2).all(|w| w[0].create_at < w[1].create_at));
        assert!(got.iter().all(|p| p.channel_id == "c1"));

        // A limit past the end is not an error.
        assert_eq!(store.posts("c1", 1000).await.unwrap().len(), 50);
        assert!(store.posts("nobody", 10).await.unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
