//! AI result cache (issue #5). The `ai_results` table already exists from the
//! initial migration, with a `UNIQUE(message_id, kind)` constraint — no new
//! migration needed here. This module just adds the query patterns for
//! reading and writing a cached result, matching the style of
//! `db::folders`/`db::messages`.

use crate::error::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// One cached AI result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResult {
    pub result: String,
    pub model: String,
}

/// Look up a cached result for `(message_id, kind)`. `None` on cache miss —
/// callers call the model and `upsert` afterward.
pub fn get(conn: &Connection, message_id: i64, kind: &str) -> Result<Option<AiResult>> {
    Ok(conn
        .query_row(
            "SELECT result, model FROM ai_results WHERE message_id = ?1 AND kind = ?2",
            params![message_id, kind],
            |r| Ok(AiResult { result: r.get(0)?, model: r.get(1)? }),
        )
        .optional()?)
}

/// Insert or replace a cached result for `(message_id, kind)`, relying on the
/// table's `UNIQUE(message_id, kind)` constraint — a re-translation (e.g.
/// after a model upgrade) overwrites the prior row rather than erroring.
pub fn upsert(conn: &Connection, message_id: i64, kind: &str, result: &str, model: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_results (message_id, kind, result, model)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(message_id, kind) DO UPDATE SET
            result = excluded.result,
            model = excluded.model,
            created_at = datetime('now')",
        params![message_id, kind, result, model],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::run_migrations_for_test;

    fn seed_message(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts (display_name, imap_host, imap_port, smtp_host, smtp_port, username)
             VALUES ('Test', 'imap.example', 993, 'smtp.example', 465, 'user@example.com')",
            [],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO folders (account_id, name, selected) VALUES (?1, 'INBOX', 1)",
            params![account_id],
        )
        .unwrap();
        let folder_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages (account_id, folder_id, uid, headers, mirror_state)
             VALUES (?1, ?2, 1, 'Subject: Bonjour', 'full')",
            params![account_id, folder_id],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn get_returns_none_on_cache_miss() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let message_id = seed_message(&conn);

        assert_eq!(get(&conn, message_id, "translation").unwrap(), None);
    }

    #[test]
    fn upsert_then_get_round_trips_the_cached_result() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let message_id = seed_message(&conn);

        upsert(&conn, message_id, "translation", "Hello", "alibayram/erurollm-9b-instruct").unwrap();

        let got = get(&conn, message_id, "translation").unwrap();
        assert_eq!(
            got,
            Some(AiResult { result: "Hello".to_string(), model: "alibayram/erurollm-9b-instruct".to_string() })
        );
    }

    #[test]
    fn upsert_overwrites_a_prior_cached_result_for_the_same_kind() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let message_id = seed_message(&conn);

        upsert(&conn, message_id, "translation", "first", "model-a").unwrap();
        upsert(&conn, message_id, "translation", "second", "model-b").unwrap();

        let got = get(&conn, message_id, "translation").unwrap().unwrap();
        assert_eq!(got.result, "second");
        assert_eq!(got.model, "model-b");
    }

    #[test]
    fn different_kinds_for_the_same_message_dont_collide() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations_for_test(&conn);
        let message_id = seed_message(&conn);

        upsert(&conn, message_id, "translation", "translated text", "model-a").unwrap();
        upsert(&conn, message_id, "summary", "summary text", "model-b").unwrap();

        assert_eq!(get(&conn, message_id, "translation").unwrap().unwrap().result, "translated text");
        assert_eq!(get(&conn, message_id, "summary").unwrap().unwrap().result, "summary text");
    }
}
