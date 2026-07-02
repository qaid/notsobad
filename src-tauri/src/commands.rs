use crate::ai::{self, ollama::OllamaError, Task};
use crate::connection::{self, AccountConfig, ValidationOutcome};
use crate::db::{
    self,
    accounts::Account,
    folders::Folder,
    messages::{MessageDetail, MessageSummary},
};
use crate::error::{AppError, Result};
use crate::keychain;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

/// Test credentials against the IMAP and SMTP servers. Stores nothing.
///
/// Made async so Tauri dispatches it off the main thread. The blocking
/// IMAP/SMTP I/O runs inside `spawn_blocking` to satisfy the async runtime.
#[tauri::command]
pub async fn validate_account(config: AccountConfig, app_password: String) -> ValidationOutcome {
    let cfg = config.clone();
    let pw = app_password.clone();
    tauri::async_runtime::spawn_blocking(move || connection::validate(&cfg, &pw))
        .await
        .expect("validation task panicked")
}

/// Validate, then persist: secret -> Keychain, non-secret config -> SQLite.
/// Refuses to save if validation fails, so we never store an unusable account.
///
/// Made async for the same reason as `validate_account`. Validation runs in
/// `spawn_blocking`; the DB insert and Keychain write happen after the await
/// so `State<AppState>` (not `Send`) is never held across an await point.
#[tauri::command]
pub async fn add_account(
    state: State<'_, AppState>,
    config: AccountConfig,
    app_password: String,
) -> Result<i64> {
    let cfg = config.clone();
    let pw = app_password.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || connection::validate(&cfg, &pw))
        .await
        .map_err(|e| AppError::Other(format!("validation task failed: {e}")))?;

    if !outcome.all_ok() {
        return Err(AppError::Other(
            "validation failed; fix the connection before saving".into(),
        ));
    }

    // State is not Send, so acquire the lock here (after the await) — no
    // async boundary crossed while holding the mutex.
    let conn = state.db.lock().expect("db mutex poisoned");
    let id = db::accounts::insert(&conn, &config)?;

    // Keychain write after DB insert. If it fails, roll back the row so we
    // never leave an account in SQLite without a corresponding secret.
    if let Err(e) = keychain::store_password(id, &app_password) {
        let _ = db::accounts::delete(&conn, id);
        return Err(e);
    }

    Ok(id)
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::accounts::list(&conn)
}

/// Sync one account's selected folders: full mirror for the last 6 months
/// per folder, metadata-only further back (PRD). Read-only against the
/// server (ADR 0003) — LIST + EXAMINE + UID SEARCH + UID FETCH
/// BODY.PEEK[...] only, never SELECT/STORE/etc.
///
/// Folder scope (#14 rework, opt-in): `LIST` still discovers and upserts
/// every folder on every call (so the sidebar picker always shows the full
/// set, including ones created on the server since last sync), but only
/// folders with `selected = true` are actually synced. INBOX defaults
/// selected on first discovery; everything else is opt-in via
/// `set_folder_selected`.
///
/// One IMAP session for the whole call (connect+login once), not one per
/// folder: discovery's `LIST` and every folder's `EXAMINE`/sync reuse the
/// same session, closed with a single `LOGOUT` at the end.
///
/// IMAP I/O runs in `spawn_blocking`; the DB lock is acquired only after each
/// await, same pattern as `add_account` (`State<AppState>` is not `Send`).
/// The live `imap::Session` is handed between two `spawn_blocking` calls
/// (discovery, then sync) with a DB round-trip in between — selection must be
/// read AFTER this call's own discovery has upserted newly-seen folders, or a
/// brand-new account's just-discovered INBOX would default to selected in
/// SQLite but never get synced on this same call.
///
/// Matches `selected` (persisted in SQLite, can outlive a folder's presence
/// on the server) against `ids_by_name` (this call's fresh `LIST` results).
/// A folder can be selected from a past sync yet absent from a later LIST —
/// deleted, renamed, or flipped `\Noselect` server-side — since discovery
/// only ever inserts folder rows, it never prunes stale selections. Skips
/// rather than indexes, so a vanished folder can't panic; a bare `map[&name]`
/// index panic here would poison `state.db`'s mutex for the rest of the
/// app's lifetime, not just fail this one sync.
fn selected_present_in_listing(
    ids_by_name: &std::collections::HashMap<String, i64>,
    selected: Vec<String>,
) -> Vec<(String, i64)> {
    selected.into_iter().filter_map(|name| ids_by_name.get(&name).map(|&id| (name, id))).collect()
}
#[tauri::command]
pub async fn sync_account(state: State<'_, AppState>, account_id: i64) -> Result<usize> {
    let cfg = {
        let conn = state.db.lock().expect("db mutex poisoned");
        db::accounts::config(&conn, account_id)?
    };
    let app_password = keychain::get_password(account_id)?;

    let discover_cfg = cfg.clone();
    let discover_pw = app_password.clone();
    let (mut session, folder_names) = tauri::async_runtime::spawn_blocking(move || {
        connection::sync::connect_and_list_folders(&discover_cfg, &discover_pw)
    })
    .await
    .map_err(|e| AppError::Other(format!("list_folders task failed: {e}")))?
    .map_err(AppError::Imap)?;

    let folder_ids_and_state: Vec<(String, i64, Option<i64>, i64)> = {
        let conn = state.db.lock().expect("db mutex poisoned");
        let mut ids_by_name = std::collections::HashMap::with_capacity(folder_names.len());
        for folder_name in &folder_names {
            let folder_id = db::folders::get_or_create(&conn, account_id, folder_name)?;
            ids_by_name.insert(folder_name.clone(), folder_id);
        }
        // Read the selected set AFTER upserting discovery's results, so a
        // brand-new account's just-discovered (and default-selected) INBOX
        // is syncable in this same call, not just from the next one.
        let selected = db::folders::selected_names(&conn, account_id)?;
        selected_present_in_listing(&ids_by_name, selected)
            .into_iter()
            .map(|(name, folder_id)| {
                let (uidvalidity, last_uid) = db::folders::uid_state(&conn, folder_id)?;
                Ok((name, folder_id, uidvalidity, last_uid))
            })
            .collect::<Result<Vec<_>>>()?
    };

    let sync_targets = folder_ids_and_state
        .iter()
        .map(|(name, _, uidvalidity, last_uid)| (name.clone(), *uidvalidity, *last_uid))
        .collect::<Vec<_>>();
    let results = tauri::async_runtime::spawn_blocking(move || -> std::result::Result<_, String> {
        let mut results = Vec::with_capacity(sync_targets.len());
        for (name, prior_uidvalidity, prior_last_uid) in sync_targets {
            let result = connection::sync::sync_inbox_with(
                &mut session,
                &name,
                prior_uidvalidity.map(|v| v as u32),
                prior_last_uid as u32,
            )?;
            results.push((name, result));
        }
        let _ = session.logout();
        Ok(results)
    })
    .await
    .map_err(|e| AppError::Other(format!("sync task failed: {e}")))?
    .map_err(AppError::Imap)?;

    let mut total = 0usize;
    for (folder_name, result) in results {
        let folder_id = folder_ids_and_state
            .iter()
            .find(|(name, ..)| name == &folder_name)
            .map(|(_, id, ..)| *id)
            .expect("synced folder was looked up from folder_ids_and_state");

        total += result.messages.len();
        let mut conn = state.db.lock().expect("db mutex poisoned");
        db::messages::upsert_messages(
            &mut conn,
            account_id,
            folder_id,
            &result.messages,
            result.uid_validity_changed,
        )?;
        db::folders::set_uid_state(&conn, folder_id, result.uidvalidity as i64, result.max_uid as i64)?;
    }
    Ok(total)
}

/// Toggle a folder's opt-in sync selection (#14 rework). Pure SQLite write —
/// issues no IMAP traffic. The frontend triggers a full `sync_account` call
/// right after turning a folder on, rather than this command syncing it
/// directly, so there's exactly one code path that talks to the server.
#[tauri::command]
pub fn set_folder_selected(
    state: State<'_, AppState>,
    account_id: i64,
    folder_name: String,
    selected: bool,
) -> Result<()> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::folders::set_selected(&conn, account_id, &folder_name, selected)
}

/// This account's tracked folders (populated by `sync_account`'s discovery).
#[tauri::command]
pub fn list_folders(state: State<'_, AppState>, account_id: i64) -> Result<Vec<Folder>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::folders::list(&conn, account_id)
}

/// Inbox list (one row per thread, newest first), scoped to INBOX only —
/// syncing other folders (#14) must not change what shows up here.
/// `account_id = None` spans all accounts' INBOXes.
#[tauri::command]
pub fn list_inbox(state: State<'_, AppState>, account_id: Option<i64>) -> Result<Vec<MessageSummary>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::messages::list_inbox(&conn, account_id)
}

/// One folder's message list (one row per thread, newest first), scoped by
/// name so non-INBOX folders (e.g. Archive) can be viewed explicitly.
#[tauri::command]
pub fn list_folder_messages(
    state: State<'_, AppState>,
    account_id: Option<i64>,
    folder_name: String,
) -> Result<Vec<MessageSummary>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::messages::list_folder_messages(&conn, account_id, &folder_name)
}

/// Full conversation for a thread, oldest first, scoped to one account.
#[tauri::command]
pub fn thread_messages(
    state: State<'_, AppState>,
    account_id: i64,
    thread_id: String,
) -> Result<Vec<MessageDetail>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::messages::thread_messages(&conn, account_id, &thread_id)
}

/// A message's body plus its html-vs-text flag, returned together so the
/// frontend never has to re-guess html-vs-text from the text itself.
#[derive(Debug, Serialize)]
pub struct MessageBody {
    pub body: String,
    pub body_is_html: bool,
}

/// A message's body, fetching it on demand from the server if this is a
/// metadata-only row that hasn't been opened yet (read-only: UID FETCH
/// BODY.PEEK[] — never sets \Seen server-side).
#[tauri::command]
pub async fn message_body(state: State<'_, AppState>, message_id: i64) -> Result<MessageBody> {
    let (account_id, uid, folder_name, existing_body, existing_is_html) = {
        let conn = state.db.lock().expect("db mutex poisoned");
        let (account_id, uid, folder_name) = db::messages::account_and_uid(&conn, message_id)?;
        let (existing_body, existing_is_html): (Option<String>, i64) = conn
            .query_row("SELECT body, body_is_html FROM messages WHERE id = ?1", [message_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        (account_id, uid, folder_name, existing_body, existing_is_html)
    };
    if let Some(body) = existing_body {
        return Ok(MessageBody { body, body_is_html: existing_is_html != 0 });
    }

    let cfg = {
        let conn = state.db.lock().expect("db mutex poisoned");
        db::accounts::config(&conn, account_id)?
    };
    let app_password = keychain::get_password(account_id)?;

    let fetched = tauri::async_runtime::spawn_blocking(move || {
        connection::sync::fetch_body(&cfg, &app_password, &folder_name, uid as u32)
    })
    .await
    .map_err(|e| AppError::Other(format!("fetch_body task failed: {e}")))?
    .map_err(AppError::Imap)?;

    let attachments_json = serde_json::to_string(&fetched.attachments).unwrap_or_default();
    let body_is_html = fetched.body_is_html;
    let conn = state.db.lock().expect("db mutex poisoned");
    db::messages::set_body(
        &conn,
        message_id,
        &db::messages::FetchedBody {
            body: fetched.body.clone(),
            body_is_html,
            snippet: fetched.snippet,
            attachments_json,
        },
    )?;
    Ok(MessageBody { body: fetched.body, body_is_html })
}

/// Result of `translate_message`.
///
/// `translated` is `None` when no real translation happened: either
/// detection decided the body was already English (Ollama never called), or
/// the model isn't pulled (`pull_hint` is `Some` in that case — `AppError`
/// serializes to a bare string over IPC, see error.rs, so this structured
/// field is how the frontend distinguishes "pull this model" from a generic
/// failure without string-matching an error message). This is deliberately
/// NOT "translated == original as a String": that shape bit the frontend
/// once already — for an HTML message with no real translation, the naive
/// fallback returned the raw HTML source as `translated`, and the "show
/// English by default" render branch put it through the plain-text renderer,
/// showing literal `<html>...` markup instead of the original rendered mail.
/// `Option<String>` makes "no translation occurred" a case the frontend must
/// handle explicitly (fall back to rendering `original` with its real
/// `body_is_html` flag) instead of a string value it can silently mishandle.
#[derive(Debug, Serialize)]
pub struct TranslationResult {
    pub translated: Option<String>,
    pub original: String,
    pub model: String,
    pub was_cached: bool,
    pub pull_hint: Option<String>,
}

/// Translate a message's body to English (issue #5), on-open (called by the
/// frontend when a thread/message is opened, not on arrival).
///
/// Cache-first: a prior `kind='translation'` row in `ai_results` is returned
/// instantly with no Ollama call. On a miss, calls Ollama (local only —
/// CLAUDE.md) via `ai::translate::translate`, which tries the configured
/// primary model first and falls back to the configured fallback if the
/// primary isn't pulled (see `ai::Task::primary_model`/`fallback_model`).
///
/// Requires the body already loaded (i.e. the frontend calls `message_body`
/// first for a meta_only message) — this command does not itself reach out
/// to IMAP, keeping the "who talks to the mail server" surface to exactly the
/// existing `message_body`/`sync_account` paths.
#[tauri::command]
pub async fn translate_message(state: State<'_, AppState>, message_id: i64) -> Result<TranslationResult> {
    let (body, body_is_html, cached) = {
        let conn = state.db.lock().expect("db mutex poisoned");
        let (body, body_is_html): (Option<String>, i64) = conn.query_row(
            "SELECT body, body_is_html FROM messages WHERE id = ?1",
            [message_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let cached = db::ai_results::get(&conn, message_id, "translation")?;
        (body, body_is_html != 0, cached)
    };
    let body = body.ok_or_else(|| {
        AppError::Other("message body not loaded yet; call message_body before translate_message".into())
    })?;

    if let Some(cached) = cached {
        return Ok(TranslationResult {
            translated: Some(cached.result),
            original: body,
            model: cached.model,
            was_cached: true,
            pull_hint: None,
        });
    }

    let body_for_task = body.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        ai::translate::translate(Task::Translate, &body_for_task, body_is_html)
    })
    .await
    .map_err(|e| AppError::Other(format!("translate task panicked: {e}")))?;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(OllamaError::ModelNotPulled { model }) => {
            return Ok(TranslationResult {
                translated: None,
                original: body,
                model: String::new(),
                was_cached: false,
                pull_hint: Some(format!("Model not pulled. Run: ollama pull {model}")),
            });
        }
        Err(OllamaError::Other(msg)) => return Err(AppError::Ai(msg)),
    };

    // `outcome.translated` is None when deterministic detection
    // (ai::translate's whatlang step) decided the body is already English —
    // Ollama was never called. Don't write a cache row for that case:
    // detection is instant and free to re-run, so there's nothing worth
    // caching. Propagate None through rather than substituting `body` as a
    // stand-in "translation" — see TranslationResult's doc comment for why
    // that substitution is the bug this shape prevents.
    let Some(translated) = outcome.translated else {
        return Ok(TranslationResult {
            translated: None,
            original: body,
            model: String::new(),
            was_cached: false,
            pull_hint: None,
        });
    };

    let conn = state.db.lock().expect("db mutex poisoned");
    db::ai_results::upsert(&conn, message_id, "translation", &translated, &outcome.model_used)?;

    Ok(TranslationResult {
        translated: Some(translated),
        original: body,
        model: outcome.model_used,
        was_cached: false,
        pull_hint: None,
    })
}

#[cfg(test)]
mod tests {
    use super::selected_present_in_listing;
    use std::collections::HashMap;

    #[test]
    fn drops_a_selected_folder_no_longer_in_the_listing() {
        let mut ids_by_name = HashMap::new();
        ids_by_name.insert("INBOX".to_string(), 1);
        ids_by_name.insert("Archive".to_string(), 2);

        // "Deleted" is selected in SQLite from a past sync but this call's
        // LIST no longer reports it (removed/renamed/\Noselect server-side).
        let selected =
            vec!["INBOX".to_string(), "Archive".to_string(), "Deleted".to_string()];

        let result = selected_present_in_listing(&ids_by_name, selected);

        assert_eq!(
            result,
            vec![("INBOX".to_string(), 1), ("Archive".to_string(), 2)],
            "must silently drop the vanished folder, not panic"
        );
    }

    #[test]
    fn keeps_every_selected_folder_still_listed() {
        let mut ids_by_name = HashMap::new();
        ids_by_name.insert("INBOX".to_string(), 1);

        let result = selected_present_in_listing(&ids_by_name, vec!["INBOX".to_string()]);

        assert_eq!(result, vec![("INBOX".to_string(), 1)]);
    }
}
