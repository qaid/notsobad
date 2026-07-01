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

/// Migrations (1-indexed, matching `user_version`) that rebuild a table
/// referenced by a live foreign key (create-copy-DROP-RENAME, per SQLite's
/// own procedure for schema changes ALTER TABLE can't express). `PRAGMA
/// foreign_keys` is a documented no-op inside a transaction, so these need
/// FK enforcement toggled off *around* the BEGIN/COMMIT, not inside the SQL
/// text — otherwise the DROP TABLE mid-rebuild fails its own FK check against
/// child tables (ai_results/labels REFERENCE messages(id)), even though the
/// rebuild is logically safe (every child row's message_id is preserved by
/// the RENAME). Both child tables are empty on today's real upgrade path
/// (their features ship in #4/#5, after this migration), so this can't
/// actually fire yet — but a startup-critical rebuild shouldn't depend on
/// that timing holding forever, so the toggle guards it regardless.
const FK_UNSAFE_MIGRATIONS: &[i64] = &[4]; // 0004_folders.sql: messages table rebuild

/// Apply any migrations newer than the DB's current `user_version`, each in its
/// own transaction, bumping `user_version` as it goes.
fn run_migrations(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current {
            let fk_unsafe = FK_UNSAFE_MIGRATIONS.contains(&version);
            if fk_unsafe {
                conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
            }
            conn.execute_batch(&format!("BEGIN;\n{sql}\n;\nPRAGMA user_version = {version};\nCOMMIT;"))?;
            if fk_unsafe {
                conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            }
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
        // by the other tests in this file. 0004 DROPs `messages` and RENAMEs
        // a replacement into place while `ai_results`/`labels` both
        // REFERENCE messages(id); on the real upgrade path both tables are
        // still empty at this point (#4/#5 haven't shipped yet), so this
        // can't fire in production today. It's still worth guarding: this is
        // a startup-critical migration, and "child table happens to be
        // empty" is a timing invariant, not a guarantee — a dev DB seeded
        // out of order, or a future migration ordering change, could violate
        // it. The seeded ai_results row here proves the DROP/rebuild
        // survives FK enforcement with a non-empty child table, not just the
        // empty-table case that's actually true today.
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
        let message_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO ai_results (message_id, kind, result, model) VALUES (?1, 'triage', '{}', 'qwen2.5:3b')",
            [message_id],
        )
        .unwrap();

        // Through run_migrations (not a raw execute_batch of MIGRATIONS[3]):
        // this is what exercises the FK_UNSAFE_MIGRATIONS toggle in the real
        // startup path (db::open), not just the migration's own SQL text.
        run_migrations(&conn).unwrap();

        // The ai_results row must survive the messages table rebuild (same
        // message id preserved across DROP/RENAME).
        let ai_result_count: i64 = conn
            .query_row("SELECT count(*) FROM ai_results WHERE message_id = ?1", [message_id], |r| r.get(0))
            .unwrap();
        assert_eq!(ai_result_count, 1, "ai_results row should survive the messages table rebuild");

        let (folder_id, name, uidvalidity, last_uid): (i64, String, i64, i64) = conn
            .query_row(
                "SELECT id, name, uidvalidity, last_uid FROM folders WHERE account_id = ?1",
                [account_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(name, "INBOX");
        assert_eq!(uidvalidity, 42);
        assert_eq!(last_uid, 7);

        let msg_folder_id: i64 = conn
            .query_row("SELECT folder_id FROM messages WHERE account_id = ?1", [account_id], |r| r.get(0))
            .unwrap();
        assert_eq!(msg_folder_id, folder_id);
    }
}
