use serde::Serialize;

/// App error surfaced to the frontend over IPC.
///
/// Variants carry only non-secret context (host, protocol, a message). Never
/// put a password or full credential in here — these get serialized to the UI
/// and may be logged. (CLAUDE.md: credentials never logged or persisted.)
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("keychain error: {0}")]
    Keychain(#[from] keyring::Error),

    #[error("IMAP connection failed: {0}")]
    Imap(String),

    #[error("SMTP connection failed: {0}")]
    #[allow(dead_code)]
    Smtp(String),

    #[error("AI request failed: {0}")]
    Ai(String),

    #[error("{0}")]
    Other(String),
}

// Serialize as a plain string so the frontend gets a readable message.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> core::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
