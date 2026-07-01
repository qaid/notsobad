//! Per-folder bookkeeping (#14). UID sync state moves from `accounts`
//! (INBOX-only, issue #3) to here: each folder has its own UIDVALIDITY/last_uid
//! since UIDs are only unique within a single mailbox.

use crate::error::Result;
use rusqlite::{OptionalExtension, Connection};
use serde::Serialize;

/// A folder row for the sidebar's folder picker. No sync bookkeeping —
/// that's internal to sync, not display.
#[derive(Debug, Serialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
}

/// Get a folder's id by (account_id, name), inserting a fresh row (UID state
/// zeroed) if it doesn't exist yet. Folder discovery (`list_folders`) and sync
/// both call this so a folder becomes trackable the first time it's seen.
pub fn get_or_create(conn: &Connection, account_id: i64, name: &str) -> Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND name = ?2",
            rusqlite::params![account_id, name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO folders (account_id, name) VALUES (?1, ?2)",
        rusqlite::params![account_id, name],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Prior UID-sync bookkeeping for a folder: (uidvalidity, last_uid).
pub fn uid_state(conn: &Connection, folder_id: i64) -> Result<(Option<i64>, i64)> {
    Ok(conn.query_row(
        "SELECT uidvalidity, last_uid FROM folders WHERE id = ?1",
        [folder_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

/// Persist UID-sync bookkeeping after a sync pass.
pub fn set_uid_state(conn: &Connection, folder_id: i64, uidvalidity: i64, last_uid: i64) -> Result<()> {
    conn.execute(
        "UPDATE folders SET uidvalidity = ?1, last_uid = ?2 WHERE id = ?3",
        rusqlite::params![uidvalidity, last_uid, folder_id],
    )?;
    Ok(())
}

/// All tracked folders for an account, alphabetical (INBOX included once synced).
pub fn list(conn: &Connection, account_id: i64) -> Result<Vec<Folder>> {
    let mut stmt = conn.prepare("SELECT id, account_id, name FROM folders WHERE account_id = ?1 ORDER BY name")?;
    let rows = stmt.query_map([account_id], |r| {
        Ok(Folder { id: r.get(0)?, account_id: r.get(1)?, name: r.get(2)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::run_migrations_for_test;

    fn setup_account(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts (display_name, imap_host, imap_port, smtp_host, smtp_port, username)
             VALUES ('Test', 'imap.example', 993, 'smtp.example', 465, 'user@example.com')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn get_or_create_is_idempotent_per_account_and_name() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let account_id = setup_account(&conn);

        let id1 = get_or_create(&conn, account_id, "Archive").unwrap();
        let id2 = get_or_create(&conn, account_id, "Archive").unwrap();
        assert_eq!(id1, id2);

        let id3 = get_or_create(&conn, account_id, "INBOX").unwrap();
        assert_ne!(id1, id3);
    }

    #[test]
    fn uid_state_roundtrips_per_folder() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let account_id = setup_account(&conn);

        let inbox = get_or_create(&conn, account_id, "INBOX").unwrap();
        let archive = get_or_create(&conn, account_id, "Archive").unwrap();

        set_uid_state(&conn, inbox, 10, 100).unwrap();
        set_uid_state(&conn, archive, 20, 5).unwrap();

        assert_eq!(uid_state(&conn, inbox).unwrap(), (Some(10), 100));
        assert_eq!(uid_state(&conn, archive).unwrap(), (Some(20), 5));
    }

    #[test]
    fn list_returns_folders_for_account_only() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let a1 = setup_account(&conn);
        let a2 = setup_account(&conn);

        get_or_create(&conn, a1, "INBOX").unwrap();
        get_or_create(&conn, a1, "Archive").unwrap();
        get_or_create(&conn, a2, "INBOX").unwrap();

        let folders = list(&conn, a1).unwrap();
        assert_eq!(folders.len(), 2);
        assert!(folders.iter().all(|f| f.account_id == a1));
    }
}
