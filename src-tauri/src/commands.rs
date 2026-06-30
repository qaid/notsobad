use crate::connection::{self, AccountConfig, ValidationOutcome};
use crate::db::{self, accounts::Account};
use crate::error::{AppError, Result};
use crate::keychain;
use crate::state::AppState;
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
