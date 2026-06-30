use std::net::TcpStream;
use std::time::Duration;
use super::AccountConfig;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// Validate an IMAP login WITHOUT mutating the mailbox.
///
/// Sequence: TLS connect -> LOGIN -> CAPABILITY -> EXAMINE INBOX (read-only) ->
/// LOGOUT. EXAMINE opens the mailbox read-only; we never SELECT, STORE, MOVE,
/// EXPUNGE, or set flags (ADR 0003). Returns Err with a non-secret message on
/// failure — never echoes the password.
///
/// Uses an explicit connect timeout (15 s) and read/write timeout (30 s) to
/// avoid hanging indefinitely on an unreachable host.
pub fn validate(cfg: &AccountConfig, app_password: &str) -> Result<(), String> {
    use std::net::ToSocketAddrs;

    let addr = (cfg.imap_host.as_str(), cfg.imap_port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS lookup for {} failed: {e}", cfg.imap_host))?
        .next()
        .ok_or_else(|| format!("no address for {}", cfg.imap_host))?;

    let tcp = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("TCP connect to {} failed: {e}", cfg.imap_host))?;
    tcp.set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| format!("set_read_timeout failed: {e}"))?;
    tcp.set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| format!("set_write_timeout failed: {e}"))?;

    let tls_connector = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS setup failed: {e}"))?;
    let tls_stream = tls_connector
        .connect(&cfg.imap_host, tcp)
        .map_err(|e| format!("TLS handshake with {} failed: {e}", cfg.imap_host))?;

    // Build a Client from the already-connected TLS stream. The imap crate's
    // Client::new reads the server greeting; login() then sends LOGIN.
    let client = imap::Client::new(tls_stream);

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
