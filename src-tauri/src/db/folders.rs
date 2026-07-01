//! Per-folder bookkeeping (#14). UID sync state moves from `accounts`
//! (INBOX-only, issue #3) to here: each folder has its own UIDVALIDITY/last_uid
//! since UIDs are only unique within a single mailbox.

use crate::error::Result;
use rusqlite::{OptionalExtension, Connection};
use serde::Serialize;

/// A folder row for the sidebar's folder picker. No UID-sync bookkeeping —
/// that's internal to sync, not display. `selected` drives opt-in sync
/// (#14 rework): only selected folders are looped over by `sync_account`.
#[derive(Debug, Serialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub selected: bool,
}

/// Get a folder's id by (account_id, name), inserting a fresh row (UID state
/// zeroed) if it doesn't exist yet. Folder discovery (`list_folders`) and sync
/// both call this so a folder becomes trackable the first time it's seen.
///
/// Default selection on first discovery: INBOX is selected (preserves
/// pre-#14-rework behavior, which already synced it); every other folder
/// defaults unselected (opt-in). This only applies to the INSERT branch — an
/// existing folder's `selected` flag is never touched here, so re-running
/// discovery doesn't clobber a user's toggle.
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
    let selected = name == "INBOX";
    conn.execute(
        "INSERT INTO folders (account_id, name, selected) VALUES (?1, ?2, ?3)",
        rusqlite::params![account_id, name, selected as i64],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Set a folder's opt-in sync selection (`set_folder_selected` command).
/// Pure SQLite — issues no IMAP traffic.
pub fn set_selected(conn: &Connection, account_id: i64, name: &str, selected: bool) -> Result<()> {
    conn.execute(
        "UPDATE folders SET selected = ?1 WHERE account_id = ?2 AND name = ?3",
        rusqlite::params![selected as i64, account_id, name],
    )?;
    Ok(())
}

/// Folder ids + names selected for sync on this account (used by
/// `sync_account` after discovery/upsert has run).
pub fn selected_names(conn: &Connection, account_id: i64) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM folders WHERE account_id = ?1 AND selected = 1 ORDER BY name")?;
    let rows = stmt.query_map([account_id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
    let mut stmt =
        conn.prepare("SELECT id, account_id, name, selected FROM folders WHERE account_id = ?1 ORDER BY name")?;
    let rows = stmt.query_map([account_id], |r| {
        Ok(Folder {
            id: r.get(0)?,
            account_id: r.get(1)?,
            name: r.get(2)?,
            selected: r.get::<_, i64>(3)? != 0,
        })
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

    /// #14 rework: on first discovery, INBOX defaults to selected (preserves
    /// today's syncing behavior) and every other folder defaults unselected
    /// (opt-in) — so a brand-new account only auto-syncs INBOX.
    #[test]
    fn get_or_create_defaults_inbox_selected_and_others_unselected() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let account_id = setup_account(&conn);

        get_or_create(&conn, account_id, "INBOX").unwrap();
        get_or_create(&conn, account_id, "Archive").unwrap();

        let folders = list(&conn, account_id).unwrap();
        let inbox = folders.iter().find(|f| f.name == "INBOX").unwrap();
        let archive = folders.iter().find(|f| f.name == "Archive").unwrap();
        assert!(inbox.selected, "INBOX must default to selected");
        assert!(!archive.selected, "non-INBOX folders must default to unselected");
    }

    #[test]
    fn set_selected_toggles_without_touching_other_folders() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let account_id = setup_account(&conn);
        get_or_create(&conn, account_id, "INBOX").unwrap();
        get_or_create(&conn, account_id, "Archive").unwrap();

        set_selected(&conn, account_id, "Archive", true).unwrap();
        let folders = list(&conn, account_id).unwrap();
        assert!(folders.iter().find(|f| f.name == "Archive").unwrap().selected);
        assert!(folders.iter().find(|f| f.name == "INBOX").unwrap().selected, "INBOX untouched by unrelated toggle");

        set_selected(&conn, account_id, "INBOX", false).unwrap();
        let folders = list(&conn, account_id).unwrap();
        assert!(!folders.iter().find(|f| f.name == "INBOX").unwrap().selected);
    }

    #[test]
    fn selected_names_returns_only_selected_folders_for_the_account() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let a1 = setup_account(&conn);
        let a2 = setup_account(&conn);

        get_or_create(&conn, a1, "INBOX").unwrap(); // selected by default
        get_or_create(&conn, a1, "Archive").unwrap(); // unselected by default
        set_selected(&conn, a1, "Archive", true).unwrap();
        get_or_create(&conn, a1, "Spam").unwrap(); // left unselected
        get_or_create(&conn, a2, "INBOX").unwrap();

        let names = selected_names(&conn, a1).unwrap();
        assert_eq!(names, vec!["Archive".to_string(), "INBOX".to_string()]);
    }
}
