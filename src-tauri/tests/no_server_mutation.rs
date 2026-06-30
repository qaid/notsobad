//! Guardrail (ADR 0003, CLAUDE.md's highest-value test): connection validation
//! must issue ZERO server-mutating commands. We drive the real client logic over
//! plaintext recording sockets and assert on the verbs the client actually sent.
//!
//! Scope note: the IMAP half guards OUR logic (EXAMINE, not SELECT — our choice).
//! The SMTP half mostly characterizes lettre, but is kept as regression coverage
//! for when #3/#5 route real send/sync traffic through the same harness.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::thread;

// Commands that mutate a mailbox or begin a send. None may appear during validation.
const IMAP_DENY: &[&str] = &[
    "STORE", "MOVE", "EXPUNGE", "APPEND", "CREATE", "DELETE", "RENAME", "COPY", "SUBSCRIBE",
    "SETACL", "SELECT", // SELECT opens read-write; we require EXAMINE instead.
];
const SMTP_DENY: &[&str] = &["MAIL", "RCPT", "DATA"];

/// First whitespace-delimited token, uppercased — the protocol verb.
fn verb(line: &str) -> String {
    line.split_whitespace().next().unwrap_or("").to_uppercase()
}

#[test]
fn imap_validation_is_read_only() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = channel::<String>();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_imap(stream, tx);
    });

    // Real client path: plaintext Client -> login -> the actual read-only checks.
    let client = imap::Client::new(TcpStream::connect(("127.0.0.1", port)).unwrap());
    let mut session = client.login("user", "pass").map_err(|(e, _)| e).unwrap();
    notsobad_lib::connection::run_readonly_checks(&mut session).unwrap();
    drop(session);
    server.join().unwrap();

    let verbs: Vec<String> = rx.try_iter().collect();
    for v in &verbs {
        assert!(!IMAP_DENY.contains(&v.as_str()), "mutating IMAP verb sent: {v} (all: {verbs:?})");
    }
    assert!(verbs.contains(&"EXAMINE".to_string()), "expected EXAMINE, got: {verbs:?}");
}

#[test]
fn smtp_validation_does_not_send() {
    use lettre::transport::smtp::SmtpTransport;
    use lettre::transport::smtp::authentication::Credentials;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = channel::<String>();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_smtp(stream, tx);
    });

    // Build the transport the same way smtp::validate does (with credentials and
    // port-based TLS selection), but target the plaintext recording server via
    // builder_dangerous. This exercises the credentialed code path: if our code
    // accidentally issued MAIL FROM, it would appear in the recorded verbs.
    // Note: this rebuilds the transport rather than calling smtp::validate directly,
    // because relay/starttls_relay force TLS negotiation that the plaintext server
    // can't satisfy. A future refactor (accept pre-built transport) would close
    // that gap fully.
    let creds = Credentials::new("user@example.com".to_string(), "secret".to_string());
    let transport = SmtpTransport::builder_dangerous("127.0.0.1")
        .port(port)
        .credentials(creds)
        .build();
    let _ = transport.test_connection(); // result irrelevant; we assert on verbs
    server.join().unwrap();

    let verbs: Vec<String> = rx.try_iter().collect();
    for v in &verbs {
        assert!(!SMTP_DENY.contains(&v.as_str()), "send-initiating SMTP verb sent: {v} (all: {verbs:?})");
    }
    // Positive check so the denylist loop can't pass vacuously on an early error.
    assert!(verbs.contains(&"EHLO".to_string()), "expected EHLO, got: {verbs:?}");
    // Confirm the credentialed path ran: AUTH must appear so a stray MAIL FROM
    // in a credentialed code path would be caught.
    assert!(verbs.contains(&"AUTH".to_string()), "expected AUTH (credentialed path), got: {verbs:?}");
}

/// Minimal plaintext IMAP server: greet, then for each tagged command echo the
/// client's tag with OK. Records each verb. EXAMINE gets a couple of untagged
/// lines so the client parses it as a real mailbox open.
fn serve_imap(stream: TcpStream, tx: Sender<String>) {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    w.write_all(b"* OK IMAP ready\r\n").unwrap();

    let mut line = String::new();
    while {
        line.clear();
        r.read_line(&mut line).unwrap_or(0) > 0
    } {
        let trimmed = line.trim_end();
        let mut parts = trimmed.splitn(2, ' ');
        let tag = parts.next().unwrap_or("*");
        let rest = parts.next().unwrap_or("");
        let v = verb(rest);
        if v.is_empty() {
            continue;
        }
        tx.send(v.clone()).unwrap();

        if v == "EXAMINE" {
            w.write_all(b"* 0 EXISTS\r\n* OK [READ-ONLY] examined\r\n").unwrap();
        } else if v == "CAPABILITY" {
            w.write_all(b"* CAPABILITY IMAP4rev1\r\n").unwrap();
        }
        w.write_all(format!("{tag} OK {v} done\r\n").as_bytes()).unwrap();

        if v == "LOGOUT" {
            break;
        }
    }
}

/// Minimal plaintext SMTP server: greet, answer EHLO with AUTH capabilities,
/// respond to AUTH with a challenge/success sequence, OK everything else, close
/// on QUIT. Records each verb (first token of each client line).
fn serve_smtp(stream: TcpStream, tx: Sender<String>) {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    w.write_all(b"220 localhost ready\r\n").unwrap();

    let mut line = String::new();
    while {
        line.clear();
        r.read_line(&mut line).unwrap_or(0) > 0
    } {
        let trimmed = line.trim_end();
        let v = verb(trimmed);
        if v.is_empty() {
            continue;
        }
        tx.send(v.clone()).unwrap();
        match v.as_str() {
            "EHLO" | "HELO" => {
                // Advertise AUTH so lettre's credentialed path proceeds.
                w.write_all(b"250-localhost\r\n250-AUTH PLAIN LOGIN\r\n250 OK\r\n").unwrap()
            }
            "AUTH" => {
                // For AUTH LOGIN lettre sends credentials in two follow-up lines
                // (base64 username then password). Respond with a 334 challenge
                // for each, then 235 authenticated.
                let sub = trimmed.split_whitespace().nth(1).unwrap_or("").to_uppercase();
                if sub == "PLAIN" {
                    // AUTH PLAIN sends credentials inline; accept immediately.
                    w.write_all(b"235 authenticated\r\n").unwrap();
                } else {
                    // AUTH LOGIN: issue two challenges then accept.
                    w.write_all(b"334 VXNlcm5hbWU6\r\n").unwrap(); // "Username:"
                    // Read username response (not a verb, so don't push to tx).
                    line.clear();
                    r.read_line(&mut line).unwrap_or(0);
                    w.write_all(b"334 UGFzc3dvcmQ6\r\n").unwrap(); // "Password:"
                    line.clear();
                    r.read_line(&mut line).unwrap_or(0);
                    w.write_all(b"235 authenticated\r\n").unwrap();
                }
            }
            "QUIT" => {
                w.write_all(b"221 bye\r\n").unwrap();
                break;
            }
            _ => w.write_all(b"250 OK\r\n").unwrap(),
        }
    }
}
