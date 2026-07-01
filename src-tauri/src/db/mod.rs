use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

pub mod accounts;
pub mod folders;
pub mod messages;

/// Ordered migrations. Index + 1 == the schema version it produces.
/// ponytail: a plain array of embedded SQL + user_version. Add a migration
/// framework only if down-migrations or branching schema history ever matter.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_messages_readpath.sql"),
    include_str!("../../migrations/0003_body_is_html.sql"),
    include_str!("../../migrations/0004_folders.sql"),
    include_str!("../../migrations/0005_folder_selection.sql"),
];

/// Open (creating if absent) the SQLite DB at `path` and run migrations.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// Test-only helper so sibling modules (db::folders, db::messages) can spin up
/// a migrated in-memory DB without duplicating the migration list.
#[cfg(test)]
pub(crate) fn run_migrations_for_test(conn: &Connection) {
    run_migrations(conn).unwrap();
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

        for t in ["accounts", "messages", "ai_results", "labels", "folders"] {
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
            "snippet", "attachments", "body_is_html",
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

    #[test]
    fn folders_migration_rebuilds_messages_with_folder_id() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('messages') WHERE name='folder_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "messages.folder_id missing");

        // messages.folder_id must be NOT NULL.
        let notnull: i64 = conn
            .query_row(
                "SELECT \"notnull\" FROM pragma_table_info('messages') WHERE name='folder_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(notnull, 1, "messages.folder_id should be NOT NULL");
    }

    #[test]
    fn folders_migration_backfills_inbox_and_remaps_existing_messages() {
        // Run only the pre-folders migrations first, insert an account + a
        // message the old way (account-scoped uid, no folder_id column can
        // exist yet), then run the folders migration and confirm: an INBOX
        // folder row was created for the account, and the pre-existing
        // message now points at it.
        //
        // FK on, matching db::open()'s real ordering (PRAGMA foreign_keys=ON,
        // then run_migrations) — NOT the bare open_in_memory() default used
        // by the other tests in this file. This guards the 0004 rebuild
        // (DROP `messages` + RENAME a replacement into place) against FK
        // enforcement in the real startup path.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        for sql in &MIGRATIONS[..3] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute("PRAGMA user_version = 3;", []).unwrap();
        conn.execute(
            "INSERT INTO accounts (display_name, imap_host, imap_port, smtp_host, smtp_port, username, uidvalidity, last_uid)
             VALUES ('Test', 'imap.example', 993, 'smtp.example', 465, 'user@example.com', 42, 7)",
            [],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages (account_id, uid, headers) VALUES (?1, 5, 'Subject: hi')",
            [account_id],
        )
        .unwrap();

        run_migrations(&conn).unwrap();

        let (folder_id, name, uidvalidity, last_uid, selected): (i64, String, i64, i64, i64) = conn
            .query_row(
                "SELECT id, name, uidvalidity, last_uid, selected FROM folders WHERE account_id = ?1",
                [account_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(name, "INBOX");
        assert_eq!(uidvalidity, 42);
        assert_eq!(last_uid, 7);
        assert_eq!(selected, 1, "backfilled INBOX must default to selected so upgrades keep syncing it");

        let msg_folder_id: i64 = conn
            .query_row("SELECT folder_id FROM messages WHERE account_id = ?1", [account_id], |r| r.get(0))
            .unwrap();
        assert_eq!(msg_folder_id, folder_id);
    }
}
