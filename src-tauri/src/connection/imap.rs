use super::AccountConfig;

/// Validate an IMAP login WITHOUT mutating the mailbox.
///
/// Sequence: TLS connect -> LOGIN -> CAPABILITY -> EXAMINE INBOX (read-only) ->
/// LOGOUT. EXAMINE opens the mailbox read-only; we never SELECT, STORE, MOVE,
/// EXPUNGE, or set flags (ADR 0003). Returns Err with a non-secret message on
/// failure — never echoes the password.
///
/// The DNS/TCP/TLS/LOGIN sequence (with connect + I/O timeouts) lives in
/// `sync::connect_and_login`, shared with the sync module so connection
/// hardening (timeouts, TLS, future STARTTLS/OAuth) only lives in one place.
pub fn validate(cfg: &AccountConfig, app_password: &str) -> Result<(), String> {
    let mut session = super::sync::connect_and_login(cfg, app_password)?;
    run_readonly_checks(&mut session)
}

/// The read-only probe sequence, generic over the transport so the guardrail
/// test can drive the SAME logic over a plaintext recording socket. This is the
/// safety-critical part (ADR 0003): CAPABILITY + EXAMINE only — never SELECT,
/// STORE, MOVE, EXPUNGE, or any flag write.
pub fn run_readonly_checks<T: std::io::Read + std::io::Write>(
    session: &mut imap::Session<T>,
) -> Result<(), String> {
    session
        .capabilities()
        .map_err(|e| format!("capability check failed: {e}"))?;

    // EXAMINE INBOX — read-only open. Deliberately NOT select() (read-write).
    session
        .examine("INBOX")
        .map_err(|e| format!("examine INBOX failed: {e}"))?;

    // Best-effort logout; a failure here doesn't invalidate a successful login.
    let _ = session.logout();
    Ok(())
}
