use mattermost_api::models::{Channel, ChannelMember, User};
use rusqlite::Connection;
use serde::de::DeserializeOwned;

use super::error::Error;
use super::handle::Store;

impl Store {
    pub async fn channels(&self) -> Result<(Vec<Channel>, Vec<ChannelMember>), Error> {
        let conn = self.conn.lock().await;
        Ok((
            all(&conn, "SELECT body FROM channels")?,
            all(&conn, "SELECT body FROM channel_members")?,
        ))
    }

    pub async fn users(&self) -> Result<Vec<User>, Error> {
        let conn = self.conn.lock().await;
        all(&conn, "SELECT body FROM users")
    }
}

/// Every row of a one-column `body` query, deserialised.
fn all<T: DeserializeOwned>(conn: &Connection, sql: &str) -> Result<Vec<T>, Error> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(serde_json::from_str(&row?)?);
    }
    Ok(out)
}
