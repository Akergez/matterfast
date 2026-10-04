use rusqlite::{params, Connection};

use super::constants::{DROP_ALL, SCHEMA, SCHEMA_VERSION};
use super::error::Error;

/// Creates the tables, or drops and recreates them if the file was written by a
/// different version of this code.
///
/// No migrations, deliberately: everything in here can be fetched again, so the
/// cost of a wrong guess is one slower launch, whereas migration code is
/// maintained forever and only ever tested on the machines that already broke.
pub(super) fn ensure_schema(conn: &Connection) -> Result<(), Error> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .ok();
    if version == Some(SCHEMA_VERSION) {
        return Ok(());
    }
    if version.is_some() {
        tracing::info!(?version, "cache schema is out of date, starting it over");
    }
    conn.execute_batch(DROP_ALL)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
        params![SCHEMA_VERSION],
    )?;
    Ok(())
}
