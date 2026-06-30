use super::AccountConfig;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::SmtpTransport;

/// Validate SMTP credentials WITHOUT sending any mail.
///
/// `test_connection()` opens the connection, runs EHLO + STARTTLS, authenticates
/// with the configured credentials, sends NOOP, then QUIT. It never issues
/// MAIL FROM / RCPT TO / DATA — so it proves the credential without beginning a
/// send (ADR 0003). Returns Err with a non-secret message on failure.
pub fn validate(cfg: &AccountConfig, app_password: &str) -> Result<(), String> {
    let creds = Credentials::new(cfg.username.clone(), app_password.to_string());

    // Port picks the TLS mode: 465 = implicit TLS (SMTPS), everything else
    // (587, 25) = STARTTLS. Covers the common providers without a UI toggle.
    let builder = if cfg.smtp_port == 465 {
        SmtpTransport::relay(&cfg.smtp_host)
    } else {
        SmtpTransport::starttls_relay(&cfg.smtp_host)
    };
    let transport = builder
        .map_err(|e| format!("SMTP setup failed: {e}"))?
        .port(cfg.smtp_port)
        .credentials(creds)
        .build();

    match transport.test_connection() {
        Ok(true) => Ok(()),
        Ok(false) => Err("server did not accept the connection".into()),
        Err(e) => Err(format!("connection failed: {e}")),
    }
}
