use crate::connection::{self, AccountConfig, ValidationOutcome};
use crate::db::{self, accounts::Account};
use crate::error::{AppError, Result};
use crate::keychain;
use crate::state::AppState;
use tauri::State;

/// Test credentials against the IMAP and SMTP servers. Stores nothing.
#[tauri::command]
pub fn validate_account(config: AccountConfig, app_password: String) -> ValidationOutcome {
    connection::validate(&config, &app_password)
}

/// Validate, then persist: secret -> Keychain, non-secret config -> SQLite.
/// Refuses to save if validation fails, so we never store an unusable account.
#[tauri::command]
pub fn add_account(
    state: State<'_, AppState>,
    config: AccountConfig,
    app_password: String,
) -> Result<i64> {
    let outcome = connection::validate(&config, &app_password);
    if !outcome.all_ok() {
        return Err(AppError::Other(
            "validation failed; fix the connection before saving".into(),
        ));
    }

    let conn = state.db.lock().expect("db mutex poisoned");
    let id = db::accounts::insert(&conn, &config)?;
    // Keychain write last: if it fails, the row exists but has no secret. Acceptable
    // for now (#3's read path can detect a missing secret); avoids holding the lock
    // across the Keychain call.
    keychain::store_password(id, &app_password)?;
    Ok(id)
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    db::accounts::list(&conn)
}
