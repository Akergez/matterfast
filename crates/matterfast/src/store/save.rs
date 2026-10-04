use mattermost_api::models::{Channel, ChannelMember, Post, User};
use rusqlite::params;

use super::error::Error;
use super::handle::Store;

impl Store {
    pub async fn save_channels(
        &self,
        channels: Vec<Channel>,
        members: Vec<ChannelMember>,
    ) -> Result<(), Error> {
        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare_cached("INSERT OR REPLACE INTO channels (id, body) VALUES (?1, ?2)")?;
            for c in &channels {
                stmt.execute(params![c.id, serde_json::to_string(c)?])?;
            }
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO channel_members (channel_id, user_id, body)
                 VALUES (?1, ?2, ?3)",
            )?;
            for m in &members {
                stmt.execute(params![m.channel_id, m.user_id, serde_json::to_string(m)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub async fn save_posts(&self, posts: Vec<Post>) -> Result<(), Error> {
        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO posts (id, channel_id, create_at, body)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            // A message still being sent is not the server's yet: its id is
            // one we made up, and the real post arrives under another.
            for p in posts.iter().filter(|p| !p.is_pending()) {
                stmt.execute(params![
                    p.id,
                    p.channel_id,
                    p.create_at,
                    serde_json::to_string(p)?
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub async fn save_users(&self, users: Vec<User>) -> Result<(), Error> {
        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare_cached("INSERT OR REPLACE INTO users (id, body) VALUES (?1, ?2)")?;
            for u in &users {
                stmt.execute(params![u.id, serde_json::to_string(u)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::test_support::{post, temp_dir};
    use super::*;

    #[tokio::test]
    async fn rows_come_back_the_way_they_went_in() {
        let dir = temp_dir("round-trip");
        let store = Store::open_at(&dir, "example.com").unwrap();

        let channel = Channel {
            id: "c1".into(),
            display_name: "Town Square".into(),
            ..Default::default()
        };
        let member = ChannelMember {
            channel_id: "c1".into(),
            user_id: "u1".into(),
            mention_count: 3,
            ..Default::default()
        };
        let user = User {
            id: "u1".into(),
            username: "ada".into(),
            ..Default::default()
        };
        store
            .save_channels(vec![channel], vec![member])
            .await
            .unwrap();
        store.save_users(vec![user]).await.unwrap();
        store.save_posts(vec![post("p1", "c1", 100)]).await.unwrap();

        let (channels, members) = store.channels().await.unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].display_name, "Town Square");
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].mention_count, 3);

        let users = store.users().await.unwrap();
        assert_eq!(users[0].username, "ada");

        let posts = store.posts("c1", 10).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].message, "message p1");

        // Reopening the same file finds it all again: this is the whole point.
        let reopened = Store::open_at(&dir, "example.com").unwrap();
        assert_eq!(reopened.posts("c1", 10).await.unwrap().len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_message_still_being_sent_is_not_kept() {
        let dir = temp_dir("pending");
        let store = Store::open_at(&dir, "example.com").unwrap();
        let mut sending = post("pending-1", "c1", 200);
        sending.pending_post_id = "pending-1".into();
        // The echo of a send carries the pending id too, under its real id.
        let mut sent = post("p2", "c1", 300);
        sent.pending_post_id = "pending-0".into();
        store
            .save_posts(vec![post("p1", "c1", 100), sending.clone(), sent])
            .await
            .unwrap();
        let ids: Vec<String> = store
            .posts("c1", 10)
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["p1", "p2"]);

        // One left behind by an older build goes when the store is opened.
        {
            let conn = store.conn.lock().await;
            conn.execute(
                "INSERT INTO posts (id, channel_id, create_at, body) VALUES (?1, ?2, ?3, ?4)",
                params![
                    sending.id,
                    sending.channel_id,
                    sending.create_at,
                    serde_json::to_string(&sending).unwrap()
                ],
            )
            .unwrap();
        }
        assert_eq!(store.posts("c1", 10).await.unwrap().len(), 3);
        let reopened = Store::open_at(&dir, "example.com").unwrap();
        assert_eq!(reopened.posts("c1", 10).await.unwrap().len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn saving_a_post_again_edits_it() {
        let dir = temp_dir("upsert");
        let store = Store::open_at(&dir, "example.com").unwrap();
        store.save_posts(vec![post("p1", "c1", 100)]).await.unwrap();

        let mut edited = post("p1", "c1", 100);
        edited.message = "fixed the typo".into();
        store.save_posts(vec![edited]).await.unwrap();

        let posts = store.posts("c1", 10).await.unwrap();
        assert_eq!(posts.len(), 1, "the edit duplicated the post");
        assert_eq!(posts[0].message, "fixed the typo");

        let _ = fs::remove_dir_all(&dir);
    }
}
