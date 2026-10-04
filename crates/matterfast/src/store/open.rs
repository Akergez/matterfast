use std::fs;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

use super::ensure_schema::ensure_schema;
use super::error::Error;
use super::handle::Store;
use super::host_of::host_of;
use super::paths::{db_path, remove_at};

impl Store {
    /// Opens (creating if needed) the database for `server`.
    pub async fn open(server: &str) -> Result<Store, Error> {
        let dir = crate::paths::data_dir().join(crate::APP_ID);
        Store::open_at(&dir, &host_of(server)?)
    }

    /// Sign-out: delete the database. The next person to use this account
    /// should not find the last one's messages.
    pub async fn clear(server: &str) -> Result<(), Error> {
        let dir = crate::paths::data_dir().join(crate::APP_ID);
        remove_at(&dir, &host_of(server)?);
        Ok(())
    }

    /// The real constructor, with the directory passed in.
    ///
    /// Separate from [`Store::open`] so the tests can point at a temporary
    /// directory: they must never touch — or delete — the user's real cache.
    pub(super) fn open_at(dir: &Path, host: &str) -> Result<Store, Error> {
        fs::create_dir_all(dir)?;
        let path = db_path(dir, host);
        // 0600 from the start. This holds messages, and SQLite would create the
        // file 0666-minus-umask; creating it ourselves first is the only way it
        // is never, even briefly, world readable. SQLite copies this mode onto
        // its journal files.
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        options.mode(0o600);
        let _created = options.open(&path)?;
        let conn = Connection::open(&path)?;
        ensure_schema(&conn)?;
        // Earlier builds filed the optimistic copy of a message being sent.
        // Nothing ever replaced it, so it came back on every launch as a
        // message forever on its way. The row's id is the pending id.
        conn.execute(
            "DELETE FROM posts WHERE json_extract(body, '$.pending_post_id') = id",
            [],
        )?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::constants::SCHEMA_VERSION;
    use super::super::test_support::{post, temp_dir};
    use super::*;
    use rusqlite::params;

    #[tokio::test]
    async fn a_file_from_another_schema_is_thrown_away() {
        let dir = temp_dir("version");
        let store = Store::open_at(&dir, "example.com").unwrap();
        store.save_posts(vec![post("p1", "c1", 100)]).await.unwrap();
        drop(store);

        // Pretend the file was written by a future version of this code.
        let conn = Connection::open(db_path(&dir, "example.com")).unwrap();
        conn.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            params![SCHEMA_VERSION + 1],
        )
        .unwrap();
        drop(conn);

        let store = Store::open_at(&dir, "example.com").unwrap();
        assert!(
            store.posts("c1", 10).await.unwrap().is_empty(),
            "the old rows survived a schema change"
        );
        // And it is usable again immediately.
        store.save_posts(vec![post("p2", "c1", 200)]).await.unwrap();
        assert_eq!(store.posts("c1", 10).await.unwrap().len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn deleting_the_database_leaves_nothing_behind() {
        let dir = temp_dir("clear");
        let store = Store::open_at(&dir, "example.com").unwrap();
        store.save_posts(vec![post("p1", "c1", 100)]).await.unwrap();
        drop(store);

        remove_at(&dir, "example.com");
        assert!(!db_path(&dir, "example.com").exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
