use crate::connection::AccountConfig;
use crate::error::Result;
use rusqlite::Connection;
use serde::Serialize;

/// A non-secret account row for the sidebar. No credential fields.
#[derive(Debug, Serialize)]
pub struct Account {
    pub id: i64,
    pub display_name: String,
    pub username: String,
    pub imap_host: String,
    pub smtp_host: String,
}

/// Insert a non-secret account config, returning its new id. The app-password
/// is NOT passed here — it goes to the Keychain separately (keychain.rs).
pub fn insert(conn: &Connection, cfg: &AccountConfig) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts (display_name, imap_host, imap_port, smtp_host, smtp_port, username)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            cfg.display_name,
            cfg.imap_host,
            cfg.imap_port,
            cfg.smtp_host,
            cfg.smtp_port,
            cfg.username,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delete an account row by id. Used to roll back an orphaned insert when the
/// Keychain write fails immediately after (see commands::add_account).
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
    Ok(())
}

/// Full connection config for an account, needed to drive sync (ports + hosts
/// the sidebar's `Account` view doesn't carry).
pub fn config(conn: &Connection, account_id: i64) -> Result<AccountConfig> {
    Ok(conn.query_row(
        "SELECT display_name, imap_host, imap_port, smtp_host, smtp_port, username
         FROM accounts WHERE id = ?1",
        [account_id],
        |r| {
            Ok(AccountConfig {
                display_name: r.get(0)?,
                imap_host: r.get(1)?,
                imap_port: r.get(2)?,
                smtp_host: r.get(3)?,
                smtp_port: r.get(4)?,
                username: r.get(5)?,
            })
        },
    )?)
}

// UID-sync bookkeeping moved to db::folders (#14) — a single account-level
// uidvalidity/last_uid pair only made sense while INBOX was the only synced
// mailbox. The accounts.uidvalidity/last_uid columns still exist (dropping
// needs a table rebuild) but are no longer read or written.

pub fn list(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, display_name, username, imap_host, smtp_host FROM accounts ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Account {
            id: r.get(0)?,
            display_name: r.get(1)?,
            username: r.get(2)?,
            imap_host: r.get(3)?,
            smtp_host: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
