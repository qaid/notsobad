use super::AccountConfig;

/// Validate an IMAP login WITHOUT mutating the mailbox.
///
/// Sequence: TLS connect -> LOGIN -> CAPABILITY -> EXAMINE INBOX (read-only) ->
/// LOGOUT. EXAMINE opens the mailbox read-only; we never SELECT, STORE, MOVE,
/// EXPUNGE, or set flags (ADR 0003). Returns Err with a non-secret message on
/// failure — never echoes the password.
pub fn validate(cfg: &AccountConfig, app_password: &str) -> Result<(), String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS setup failed: {e}"))?;

    // imap::connect does the TLS handshake; .login() sends LOGIN.
    let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), &cfg.imap_host, &tls)
        .map_err(|e| format!("connect to {}: {e}", cfg.imap_host))?;

    let mut session = client
        .login(&cfg.username, app_password)
        // .login returns (err, client) on failure; keep only the error text.
        .map_err(|(e, _client)| format!("login failed: {e}"))?;

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
