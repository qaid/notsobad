use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

pub mod accounts;
pub mod messages;

/// Ordered migrations. Index + 1 == the schema version it produces.
/// ponytail: a plain array of embedded SQL + user_version. Add a migration
/// framework only if down-migrations or branching schema history ever matter.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_messages_readpath.sql"),
];

/// Open (creating if absent) the SQLite DB at `path` and run migrations.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// Apply any migrations newer than the DB's current `user_version`, each in its
/// own transaction, bumping `user_version` as it goes.
fn run_migrations(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current {
            conn.execute_batch(&format!("BEGIN;\n{sql}\n;\nPRAGMA user_version = {version};\nCOMMIT;"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_create_schema_and_are_idempotent() {
        // ponytail: in-memory DB, no fixtures. Fails if a migration is malformed
        // or the version-gating regresses (re-running would error on CREATE TABLE).
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // idempotent: no-op second time

        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);

        for t in ["accounts", "messages", "ai_results", "labels"] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {t} missing");
        }
    }

    #[test]
    fn readpath_migration_adds_expected_columns() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        for col in [
            "subject", "from_addr", "from_name", "seen", "in_reply_to", "refs", "thread_id",
            "snippet", "attachments",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM pragma_table_info('messages') WHERE name=?1",
                    [col],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "messages.{col} missing");
        }

        for col in ["uidvalidity", "last_uid"] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM pragma_table_info('accounts') WHERE name=?1",
                    [col],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "accounts.{col} missing");
        }
    }
}
