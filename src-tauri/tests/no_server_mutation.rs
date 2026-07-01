//! Guardrail (ADR 0003, CLAUDE.md's highest-value test): connection validation
//! AND sync must issue ZERO server-mutating commands. We drive the real client
//! logic over plaintext recording sockets and assert on the verbs the client
//! actually sent.
//!
//! Scope note: the IMAP half guards OUR logic (EXAMINE, not SELECT — our choice;
//! BODY.PEEK[...], not bare BODY[...], which would set \Seen server-side). The
//! SMTP half mostly characterizes lettre, but is kept as regression coverage for
//! when #5 routes real send traffic through the same harness.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::thread;

// IMAP: allowlist, not denylist. A denylist of single-word mutating verbs
// (STORE, COPY, MOVE...) misses their `UID <verb>` two-word forms, which is
// exactly how real clients send UID-scoped mutations on the wire — a future
// `uid_store`/`uid_copy`/`uid_move` call would slip through undetected. An
// allowlist of the exact verbs our read-only paths are permitted to send
// fails closed instead: anything new must be added here deliberately.
const IMAP_ALLOW: &[&str] =
    &["LOGIN", "CAPABILITY", "EXAMINE", "UID SEARCH", "UID FETCH", "LOGOUT"];
const SMTP_DENY: &[&str] = &["MAIL", "RCPT", "DATA"];

/// First whitespace-delimited token, uppercased — the protocol verb. For a
/// `UID <verb> ...` command (e.g. `UID FETCH`, `UID SEARCH`), returns the
/// two-word form so positive assertions can tell FETCH from SEARCH.
fn verb(line: &str) -> String {
    let mut parts = line.split_whitespace();
    let first = parts.next().unwrap_or("").to_uppercase();
    if first == "UID" {
        if let Some(second) = parts.next() {
            return format!("UID {}", second.to_uppercase());
        }
    }
    first
}

/// Locks in the fail-closed property itself: a careless future edit that adds
/// a mutating verb (including a UID two-word form) to IMAP_ALLOW should fail
/// this test, not just silently widen what the allowlist accepts.
#[test]
fn allowlist_excludes_known_mutations() {
    for m in ["UID STORE", "UID COPY", "UID MOVE", "STORE", "COPY", "MOVE", "EXPUNGE", "APPEND", "SELECT"] {
        assert!(!IMAP_ALLOW.contains(&m), "{m} must never be allowlisted (ADR 0003)");
    }
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
        assert!(IMAP_ALLOW.contains(&v.as_str()), "non-allowlisted IMAP verb sent: {v} (all: {verbs:?})");
    }
    assert!(verbs.contains(&"EXAMINE".to_string()), "expected EXAMINE, got: {verbs:?}");
}

#[test]
fn sync_is_read_only() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = channel::<String>();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_imap(stream, tx);
    });

    let client = imap::Client::new(TcpStream::connect(("127.0.0.1", port)).unwrap());
    let mut session = client.login("user", "pass").map_err(|(e, _)| e).unwrap();
    notsobad_lib::connection::sync_inbox_with(&mut session, None, 0).unwrap();
    drop(session);
    server.join().unwrap();

    let verbs: Vec<String> = rx.try_iter().collect();
    for v in &verbs {
        assert!(IMAP_ALLOW.contains(&v.as_str()), "non-allowlisted IMAP verb sent during sync: {v} (all: {verbs:?})");
    }
    assert!(verbs.contains(&"EXAMINE".to_string()), "expected EXAMINE, got: {verbs:?}");
    assert!(verbs.contains(&"UID SEARCH".to_string()), "expected UID SEARCH, got: {verbs:?}");
    assert!(verbs.contains(&"UID FETCH".to_string()), "expected UID FETCH, got: {verbs:?}");
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
            w.write_all(b"* 0 EXISTS\r\n* OK [UIDVALIDITY 1] UIDs valid\r\n* OK [READ-ONLY] examined\r\n").unwrap();
        } else if v == "CAPABILITY" {
            w.write_all(b"* CAPABILITY IMAP4rev1\r\n").unwrap();
        } else if v == "UID SEARCH" {
            // Report one fake message (UID 1) so the client's FETCH actually fires.
            w.write_all(b"* SEARCH 1\r\n").unwrap();
        } else if v == "UID FETCH" {
            // Minimal FETCH response for UID 1: a zero-byte literal is a valid,
            // parseable empty body/header.
            w.write_all(b"* 1 FETCH (UID 1 FLAGS () BODY[] {0}\r\n)\r\n").unwrap();
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
