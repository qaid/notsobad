//! Mail connection layer. CONCRETE IMAP/SMTP only (issue #2).
//!
//! This is the seam other backends (Gmail API, Exchange) plug into at #7 — but
//! there is NO trait yet: a one-impl trait would just encode IMAP-isms. The
//! "room for later" is this module boundary: callers see only the domain types
//! below, never `imap`/`lettre` types. The trait gets extracted at #7 from two
//! real backends. (ponytail: concrete now, abstract when a second impl exists.)
//!
//! SAFETY (ADR 0003): validation NEVER mutates the server. IMAP uses EXAMINE
//! (read-only), never SELECT/STORE/etc. SMTP authenticates and stops — it never
//! issues MAIL FROM/RCPT TO/DATA. The guardrail test enforces this.

mod imap;
mod smtp;

// Exposed for the no-server-mutation guardrail test (tests/no_server_mutation.rs),
// which drives the real read-only IMAP sequence over a plaintext recording socket.
#[doc(hidden)]
pub use imap::run_readonly_checks;

use serde::{Deserialize, Serialize};

/// Non-secret account configuration from the wizard form.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    pub display_name: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub username: String,
}

/// Per-protocol validation result. `ok` true = login succeeded read-only.
#[derive(Debug, Serialize)]
pub struct ProtocolResult {
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ValidationOutcome {
    pub imap: ProtocolResult,
    pub smtp: ProtocolResult,
}

impl ValidationOutcome {
    pub fn all_ok(&self) -> bool {
        self.imap.ok && self.smtp.ok
    }
}

/// Validate credentials against both servers without mutating either.
/// Runs the (blocking) IMAP and SMTP checks; callers invoke from a blocking
/// context (Tauri commands run on a worker thread).
pub fn validate(cfg: &AccountConfig, app_password: &str) -> ValidationOutcome {
    ValidationOutcome {
        imap: to_result(imap::validate(cfg, app_password)),
        smtp: to_result(smtp::validate(cfg, app_password)),
    }
}

fn to_result(r: Result<(), String>) -> ProtocolResult {
    match r {
        Ok(()) => ProtocolResult { ok: true, error: None },
        Err(e) => ProtocolResult { ok: false, error: Some(e) },
    }
}
