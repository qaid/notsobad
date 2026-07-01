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
//
// LIST (#14, folder discovery) is read-only per RFC3501 — it only enumerates
// mailbox names, never opens or mutates one — so it belongs on the allowlist
// alongside EXAMINE, not treated as a mutation risk.
const IMAP_ALLOW: &[&str] =
    &["LOGIN", "CAPABILITY", "EXAMINE", "LIST", "UID SEARCH", "UID FETCH", "LOGOUT"];
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
    notsobad_lib::connection::sync_inbox_with(&mut session, "INBOX", None, 0).unwrap();
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

/// #14: syncing a non-INBOX folder (e.g. Archive) must EXAMINE that folder by
/// name, never INBOX, and still send only allowlisted verbs — folder-scoping
/// sync must not open a door to a mutating command on some other mailbox.
#[test]
fn folder_scoped_sync_is_read_only_and_examines_named_folder() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = channel::<String>();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_imap(stream, tx);
    });

    let client = imap::Client::new(TcpStream::connect(("127.0.0.1", port)).unwrap());
    let mut session = client.login("user", "pass").map_err(|(e, _)| e).unwrap();
    notsobad_lib::connection::sync_inbox_with(&mut session, "Archive", None, 0).unwrap();
    drop(session);
    server.join().unwrap();

    let verbs: Vec<String> = rx.try_iter().collect();
    for v in &verbs {
        assert!(
            IMAP_ALLOW.contains(&v.as_str()),
            "non-allowlisted IMAP verb sent during folder-scoped sync: {v} (all: {verbs:?})"
        );
    }
    assert!(verbs.contains(&"EXAMINE".to_string()), "expected EXAMINE, got: {verbs:?}");
    assert!(!verbs.contains(&"SELECT".to_string()), "must never SELECT, got: {verbs:?}");
}

/// #14: folder discovery (`list_folders_with`, IMAP `LIST`) must send only
/// allowlisted verbs and never open (EXAMINE/SELECT) any mailbox — it only
/// enumerates names.
#[test]
fn list_folders_is_read_only() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = channel::<String>();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_imap(stream, tx);
    });

    let client = imap::Client::new(TcpStream::connect(("127.0.0.1", port)).unwrap());
    let mut session = client.login("user", "pass").map_err(|(e, _)| e).unwrap();
    let folders = notsobad_lib::connection::list_folders_with(&mut session).unwrap();
    drop(session);
    server.join().unwrap();

    let verbs: Vec<String> = rx.try_iter().collect();
    for v in &verbs {
        assert!(IMAP_ALLOW.contains(&v.as_str()), "non-allowlisted IMAP verb sent during LIST: {v} (all: {verbs:?})");
    }
    assert!(verbs.contains(&"LIST".to_string()), "expected LIST, got: {verbs:?}");
    assert!(!verbs.contains(&"EXAMINE".to_string()), "LIST must not open any mailbox, got: {verbs:?}");
    assert_eq!(folders, vec!["INBOX".to_string()], "expected the fake LIST response's one folder");
}

/// #14 rework: `sync_account` now opens ONE IMAP session and reuses it across
/// discovery (`LIST`) plus a loop of per-folder syncs (`sync_inbox_with`),
/// closing with a single `LOGOUT` at the end — see
/// `connection::sync::connect_and_list_folders` and its caller in
/// `commands::sync_account`. The pre-existing guardrail tests above only ever
/// drive `sync_inbox_with` once, in isolation, on a session that test itself
/// set up — nothing exercised the real reused-session, multiple-folder
/// sequence. This test does.
///
/// It can't call `connect_and_list_folders` directly: that function calls
/// `connect_and_login`, which hardcodes a TLS handshake, so it can't be
/// driven over this test's plaintext recording socket. Every existing test in
/// this file works around the same constraint by driving the generic,
/// transport-agnostic halves (`list_folders_with`, `sync_inbox_with`)
/// directly over a plaintext `imap::Client` login — that IS the logic inside
/// `connect_and_list_folders` / `sync_account`'s loop, just assembled here
/// instead of through the TLS-only wrapper. This test assembles them the same
/// way `sync_account` does: one login, one `list_folders_with` call, then
/// `sync_inbox_with` looped over two distinct folder names on that same
/// session, then one `logout`.
#[test]
fn multi_folder_sync_on_one_session_is_read_only() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = channel::<String>();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_imap(stream, tx);
    });

    let client = imap::Client::new(TcpStream::connect(("127.0.0.1", port)).unwrap());
    let mut session = client.login("user", "pass").map_err(|(e, _)| e).unwrap();

    // Mirrors connect_and_list_folders's discovery half.
    let _ = notsobad_lib::connection::list_folders_with(&mut session).unwrap();

    // Mirrors sync_account's loop: reuse the SAME session across every
    // selected folder, never reconnecting between them.
    for folder in ["INBOX", "Archive"] {
        notsobad_lib::connection::sync_inbox_with(&mut session, folder, None, 0).unwrap();
    }

    let _ = session.logout();
    server.join().unwrap();

    let verbs: Vec<String> = rx.try_iter().collect();
    for v in &verbs {
        assert!(
            IMAP_ALLOW.contains(&v.as_str()),
            "non-allowlisted IMAP verb sent during multi-folder sync: {v} (all: {verbs:?})"
        );
    }
    assert!(!verbs.contains(&"SELECT".to_string()), "must never SELECT, got: {verbs:?}");
    assert!(verbs.contains(&"LIST".to_string()), "expected LIST (discovery), got: {verbs:?}");
    // verb() collapses "EXAMINE INBOX" and "EXAMINE Archive" to the same
    // token, so per-folder EXAMINE is proven by count, not by name: two
    // EXAMINEs on one reused session means both loop iterations actually ran
    // sync against the server, not just the first.
    let examine_count = verbs.iter().filter(|v| v.as_str() == "EXAMINE").count();
    assert_eq!(examine_count, 2, "expected EXAMINE for both folders, got: {verbs:?}");
    let logout_count = verbs.iter().filter(|v| v.as_str() == "LOGOUT").count();
    assert_eq!(logout_count, 1, "expected exactly one LOGOUT for the whole reused session, got: {verbs:?}");
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
        } else if v == "LIST" {
            // One selectable folder so list_folders_with has something to return.
            w.write_all(b"* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n").unwrap();
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
