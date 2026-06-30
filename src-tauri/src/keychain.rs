use crate::error::Result;
use keyring::Entry;

const SERVICE: &str = "com.qaid.notsobad";

/// Keychain key for an account's app-password: stable, derived from account id.
fn key(account_id: i64) -> String {
    format!("account-{account_id}")
}

/// Store an account's app-password in the macOS Keychain. The secret lives ONLY
/// here — never in SQLite, never logged (CLAUDE.md). Write-only at this stage;
/// the read path arrives with sync (#3).
pub fn store_password(account_id: i64, app_password: &str) -> Result<()> {
    Entry::new(SERVICE, &key(account_id))?.set_password(app_password)?;
    Ok(())
}
