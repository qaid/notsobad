use crate::connection::sync::{thread_id_for, FetchedMessage};
use crate::error::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

/// One row in the inbox list: the latest message of a thread.
#[derive(Debug, Serialize)]
pub struct MessageSummary {
    pub id: i64,
    pub account_id: i64,
    pub thread_id: String,
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub subject: Option<String>,
    pub snippet: Option<String>,
    pub received_at: Option<String>,
    pub seen: bool,
}

/// One message within a thread, full detail for the reader view.
#[derive(Debug, Serialize)]
pub struct MessageDetail {
    pub id: i64,
    pub account_id: i64,
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub subject: Option<String>,
    pub headers: String,
    pub body: Option<String>,
    pub body_is_html: bool,
    pub received_at: Option<String>,
    pub seen: bool,
    pub mirror_state: String,
    pub uid: i64,
}

/// Insert or update synced messages in one transaction, keyed by (folder_id, uid).
/// If `replace_folder` is true, this folder's existing rows are cleared first
/// (UIDVALIDITY changed — the prior mirror is no longer addressable by UID).
/// Scoped to the folder (not the whole account) so re-syncing one folder after
/// a UIDVALIDITY reset never wipes another folder's mirror.
pub fn upsert_messages(
    conn: &mut Connection,
    account_id: i64,
    folder_id: i64,
    messages: &[FetchedMessage],
    replace_folder: bool,
) -> Result<()> {
    let tx = conn.transaction()?;
    if replace_folder {
        tx.execute("DELETE FROM messages WHERE folder_id = ?1", [folder_id])?;
    }
    {
        let mut stmt = tx.prepare(
            "INSERT INTO messages
                (account_id, folder_id, uid, message_id, headers, body, body_is_html, mirror_state, received_at,
                 subject, from_addr, from_name, seen, in_reply_to, refs, thread_id,
                 snippet, attachments)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
             ON CONFLICT(folder_id, uid) DO UPDATE SET
                message_id = excluded.message_id,
                headers = excluded.headers,
                body = COALESCE(excluded.body, messages.body),
                body_is_html = excluded.body_is_html,
                mirror_state = excluded.mirror_state,
                received_at = excluded.received_at,
                subject = excluded.subject,
                from_addr = excluded.from_addr,
                from_name = excluded.from_name,
                seen = excluded.seen,
                in_reply_to = excluded.in_reply_to,
                refs = excluded.refs,
                thread_id = excluded.thread_id,
                snippet = COALESCE(excluded.snippet, messages.snippet),
                attachments = excluded.attachments",
        )?;
        for m in messages {
            let refs_json = serde_json::to_string(&m.references).unwrap_or_default();
            let attachments_json = serde_json::to_string(&m.attachments).unwrap_or_default();
            stmt.execute(params![
                account_id,
                folder_id,
                m.uid,
                m.message_id,
                m.headers_raw,
                m.body,
                m.body_is_html as i64,
                m.mirror_state,
                m.received_at,
                m.subject,
                m.from_addr,
                m.from_name,
                m.seen as i64,
                m.in_reply_to,
                refs_json,
                thread_id_for(m),
                m.snippet,
                attachments_json,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Inbox list: one row per thread (its latest message), newest first, scoped
/// to the INBOX folder only. `account_id = None` spans all accounts' INBOXes.
///
/// Deliberately INBOX-only, not folder-spanning: syncing Archive/Sent/etc.
/// (#14) must not pollute the inbox view — that's a regression on #3's
/// acceptance criteria, not a feature. Non-INBOX folders are reached via
/// `list_folder_messages` instead.
pub fn list_inbox(conn: &Connection, account_id: Option<i64>) -> Result<Vec<MessageSummary>> {
    list_folder_messages(conn, account_id, "INBOX")
}

/// One row per thread (its latest message), newest first, scoped to a single
/// named folder. `account_id = None` spans all accounts' folders of that name.
pub fn list_folder_messages(
    conn: &Connection,
    account_id: Option<i64>,
    folder_name: &str,
) -> Result<Vec<MessageSummary>> {
    // received_at can be NULL for a message with an unparseable Date header;
    // `HAVING received_at = MAX(received_at)` would drop that whole thread
    // (NULL = anything is NULL, never true). COALESCE to the epoch instead so
    // a thread is never silently dropped from the inbox.
    let sql = "SELECT m.id, m.account_id, m.thread_id, m.from_name, m.from_addr, m.subject, m.snippet,
                      m.received_at, m.seen
               FROM messages m
               JOIN folders f ON f.id = m.folder_id
               WHERE f.name = ?2 AND (?1 IS NULL OR m.account_id = ?1)
               AND m.id IN (
                   SELECT m2.id FROM messages m2
                   JOIN folders f2 ON f2.id = m2.folder_id
                   WHERE f2.name = ?2 AND (?1 IS NULL OR m2.account_id = ?1)
                   GROUP BY m2.account_id, m2.thread_id
                   HAVING COALESCE(m2.received_at, '0000-00-00') = MAX(COALESCE(m2.received_at, '0000-00-00'))
               )
               ORDER BY m.received_at DESC";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![account_id, folder_name], |r| {
        Ok(MessageSummary {
            id: r.get(0)?,
            account_id: r.get(1)?,
            thread_id: r.get(2)?,
            from_name: r.get(3)?,
            from_addr: r.get(4)?,
            subject: r.get(5)?,
            snippet: r.get(6)?,
            received_at: r.get(7)?,
            seen: r.get::<_, i64>(8)? != 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Full conversation for a thread, oldest first. Scoped to `account_id` so two
/// accounts that happen to produce the same thread_id (mailing list copy,
/// self-CC, shared References chain) never mix messages in one reader view.
pub fn thread_messages(conn: &Connection, account_id: i64, thread_id: &str) -> Result<Vec<MessageDetail>> {
    let mut stmt = conn.prepare(
        "SELECT id, account_id, from_name, from_addr, subject, headers, body, body_is_html,
                received_at, seen, mirror_state, uid
         FROM messages WHERE account_id = ?1 AND thread_id = ?2 ORDER BY received_at ASC",
    )?;
    let rows = stmt.query_map(params![account_id, thread_id], |r| {
        Ok(MessageDetail {
            id: r.get(0)?,
            account_id: r.get(1)?,
            from_name: r.get(2)?,
            from_addr: r.get(3)?,
            subject: r.get(4)?,
            headers: r.get(5)?,
            body: r.get(6)?,
            body_is_html: r.get::<_, i64>(7)? != 0,
            received_at: r.get(8)?,
            seen: r.get::<_, i64>(9)? != 0,
            mirror_state: r.get(10)?,
            uid: r.get(11)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A lazily-fetched body plus the metadata that comes free from the same parse
/// (snippet, attachments, html-vs-text) — populating them here means an
/// on-demand fetch doesn't leave those columns permanently empty just because
/// the message happened to sync as metadata-only first.
pub struct FetchedBody {
    pub body: String,
    pub body_is_html: bool,
    pub snippet: Option<String>,
    pub attachments_json: String,
}

/// Persist a lazily-fetched body for a metadata-only message.
pub fn set_body(conn: &Connection, message_id: i64, fetched: &FetchedBody) -> Result<()> {
    conn.execute(
        "UPDATE messages
         SET body = ?1, body_is_html = ?2, mirror_state = 'full', snippet = ?3, attachments = ?4
         WHERE id = ?5",
        params![
            fetched.body,
            fetched.body_is_html as i64,
            fetched.snippet,
            fetched.attachments_json,
            message_id
        ],
    )?;
    Ok(())
}

/// Look up an account id + IMAP UID + folder name for a stored message row
/// (needed to EXAMINE the right mailbox and fetch its body on demand — a
/// non-INBOX message's UID is only meaningful within its own folder).
pub fn account_and_uid(conn: &Connection, message_id: i64) -> Result<(i64, i64, String)> {
    Ok(conn.query_row(
        "SELECT m.account_id, m.uid, f.name
         FROM messages m JOIN folders f ON f.id = m.folder_id
         WHERE m.id = ?1",
        [message_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{folders, run_migrations_for_test};

    /// AC2-critical wiring (#14): opening a metadata-only message stored in a
    /// non-INBOX folder (e.g. Archive) must fetch its body against ITS OWN
    /// folder, not INBOX — the UID is only meaningful within the folder it
    /// was synced from. `commands::message_body` relies on `account_and_uid`
    /// returning the real folder name for this to work; this test locks that
    /// contract in without needing a live IMAP server.
    #[test]
    fn account_and_uid_returns_the_messages_own_non_inbox_folder() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        conn.execute(
            "INSERT INTO accounts (display_name, imap_host, imap_port, smtp_host, smtp_port, username)
             VALUES ('Test', 'imap.example', 993, 'smtp.example', 465, 'user@example.com')",
            [],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        let archive_id = folders::get_or_create(&conn, account_id, "Archive").unwrap();

        conn.execute(
            "INSERT INTO messages (account_id, folder_id, uid, headers, mirror_state)
             VALUES (?1, ?2, 9, 'Subject: old', 'meta_only')",
            params![account_id, archive_id],
        )
        .unwrap();
        let message_id = conn.last_insert_rowid();

        let (got_account_id, got_uid, got_folder_name) = account_and_uid(&conn, message_id).unwrap();
        assert_eq!(got_account_id, account_id);
        assert_eq!(got_uid, 9);
        assert_eq!(got_folder_name, "Archive", "must resolve the message's own folder, not INBOX");
    }
}
