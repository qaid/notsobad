//! INBOX sync (#3): read-only mirror of recent + metadata-only older mail.
//!
//! SAFETY (ADR 0003): EXAMINE (never SELECT), UID SEARCH, UID FETCH with
//! BODY.PEEK[...] (never bare BODY[...], which would set \Seen server-side).
//! No STORE/MOVE/EXPUNGE/COPY ever. The guardrail test drives this module's
//! logic over a plaintext recording socket and asserts on the verbs sent.

use super::AccountConfig;
use mail_parser::{HeaderValue, MessageParser, MimeHeaders};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const MIRROR_WINDOW_DAYS: i64 = 183; // ~6 months (PRD)

/// One attachment's display metadata — index only, never the bytes.
#[derive(Debug, serde::Serialize)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mime: String,
    pub size: usize,
}

/// A single synced message, parsed and ready to upsert into SQLite.
#[derive(Debug)]
pub struct FetchedMessage {
    pub uid: u32,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub received_at: Option<String>, // RFC3339
    pub seen: bool,
    pub headers_raw: String,
    pub body: Option<String>,   // None for metadata-only rows
    pub snippet: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub mirror_state: &'static str, // "full" | "meta_only"
}

/// Result of one INBOX sync pass.
pub struct SyncResult {
    pub messages: Vec<FetchedMessage>,
    pub uidvalidity: u32,
    pub max_uid: u32,
    /// True if the server's UIDVALIDITY changed since last sync — caller must
    /// discard the prior mirror for this account before inserting these rows.
    pub uid_validity_changed: bool,
}

/// Connect, log in, and EXAMINE INBOX (read-only). Shared by validate() and sync.
fn connect_and_examine(
    cfg: &AccountConfig,
    app_password: &str,
) -> Result<imap::Session<native_tls::TlsStream<TcpStream>>, String> {
    use std::net::ToSocketAddrs;

    let addr = (cfg.imap_host.as_str(), cfg.imap_port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS lookup for {} failed: {e}", cfg.imap_host))?
        .next()
        .ok_or_else(|| format!("no address for {}", cfg.imap_host))?;

    let tcp = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("TCP connect to {} failed: {e}", cfg.imap_host))?;
    tcp.set_read_timeout(Some(IO_TIMEOUT)).map_err(|e| format!("set_read_timeout failed: {e}"))?;
    tcp.set_write_timeout(Some(IO_TIMEOUT)).map_err(|e| format!("set_write_timeout failed: {e}"))?;

    let tls_connector = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS setup failed: {e}"))?;
    let tls_stream = tls_connector
        .connect(&cfg.imap_host, tcp)
        .map_err(|e| format!("TLS handshake with {} failed: {e}", cfg.imap_host))?;

    let client = imap::Client::new(tls_stream);
    let mut session = client
        .login(&cfg.username, app_password)
        .map_err(|(e, _client)| format!("login failed: {e}"))?;

    session.examine("INBOX").map_err(|e| format!("examine INBOX failed: {e}"))?;
    Ok(session)
}

/// Sync INBOX: full mirror for the last 6 months, metadata-only further back.
/// `prior_uidvalidity`/`prior_last_uid` come from `accounts` and drive the
/// incremental fetch; pass `prior_last_uid = 0` for a first sync.
pub fn sync_inbox(
    cfg: &AccountConfig,
    app_password: &str,
    prior_uidvalidity: Option<u32>,
    prior_last_uid: u32,
) -> Result<SyncResult, String> {
    let mut session = connect_and_examine(cfg, app_password)?;
    sync_inbox_with(&mut session, prior_uidvalidity, prior_last_uid)
}

/// The sync logic, generic over the transport so the guardrail test can drive
/// it over a plaintext recording socket (same pattern as run_readonly_checks).
pub fn sync_inbox_with<T: Read + Write>(
    session: &mut imap::Session<T>,
    prior_uidvalidity: Option<u32>,
    prior_last_uid: u32,
) -> Result<SyncResult, String> {
    let mailbox = session.examine("INBOX").map_err(|e| format!("examine INBOX failed: {e}"))?;
    let uidvalidity = mailbox.uid_validity.unwrap_or(0);
    let uid_validity_changed = prior_uidvalidity.is_some_and(|v| v != uidvalidity);
    let since_uid = if uid_validity_changed { 0 } else { prior_last_uid };

    let since_date = imap_search_date(-MIRROR_WINDOW_DAYS);

    let mut messages = Vec::new();
    let mut max_uid = since_uid;

    // Full window: recent mail above our last-synced UID, full body.
    let recent_query = format!("SINCE {since_date} UID {}:*", since_uid + 1);
    let recent_uids = session.uid_search(&recent_query).map_err(|e| format!("uid search failed: {e}"))?;
    if !recent_uids.is_empty() {
        let uid_set = uid_list(&recent_uids);
        let fetches = session
            .uid_fetch(&uid_set, "(FLAGS BODY.PEEK[])")
            .map_err(|e| format!("uid fetch failed: {e}"))?;
        for f in fetches.iter() {
            if let Some(uid) = f.uid {
                max_uid = max_uid.max(uid);
                let seen = f.flags().iter().any(|fl| matches!(fl, imap::types::Flag::Seen));
                if let Some(raw) = f.body() {
                    messages.push(parse_full(uid, seen, raw));
                }
            }
        }
    }

    // Backfill: older mail, headers only, no body — metadata-only mirror.
    // Only on a first sync or after a UIDVALIDITY reset: older mail doesn't
    // change, so re-running this every incremental sync would re-fetch the
    // same headers for nothing. ponytail: backfill is all-or-nothing per
    // epoch, not incrementally UID-gated; fine while INBOX is the only folder
    // and 6 months is the only window. Add a `backfilled_before_uid` column
    // if a mailbox's older mail needs to grow incrementally.
    if since_uid == 0 {
        let older_query = format!("BEFORE {since_date}");
        let older_uids = session.uid_search(&older_query).map_err(|e| format!("uid search failed: {e}"))?;
        if !older_uids.is_empty() {
            let uid_set = uid_list(&older_uids);
            let fetches = session
                .uid_fetch(&uid_set, "(FLAGS BODY.PEEK[HEADER])")
                .map_err(|e| format!("uid fetch failed: {e}"))?;
            for f in fetches.iter() {
                if let Some(uid) = f.uid {
                    let seen = f.flags().iter().any(|fl| matches!(fl, imap::types::Flag::Seen));
                    if let Some(raw) = f.header() {
                        messages.push(parse_meta(uid, seen, raw));
                    }
                }
            }
        }
    }

    let _ = session.logout();
    Ok(SyncResult { messages, uidvalidity, max_uid, uid_validity_changed })
}

/// Fetch a single message's full body on demand (older meta-only message opened).
pub fn fetch_body(cfg: &AccountConfig, app_password: &str, uid: u32) -> Result<String, String> {
    let mut session = connect_and_examine(cfg, app_password)?;
    let fetches = session
        .uid_fetch(uid.to_string(), "BODY.PEEK[]")
        .map_err(|e| format!("uid fetch failed: {e}"))?;
    let raw = fetches
        .iter()
        .find_map(|f| f.body())
        .ok_or_else(|| format!("no body returned for uid {uid}"))?;
    let parsed = parse_full(uid, true, raw);
    let _ = session.logout();
    parsed.body.ok_or_else(|| "message body could not be parsed".to_string())
}

fn uid_list(uids: &std::collections::HashSet<u32>) -> String {
    let mut sorted: Vec<u32> = uids.iter().copied().collect();
    sorted.sort_unstable();
    sorted.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

/// IMAP SEARCH date (RFC3501 `date-text`, e.g. "01-Jan-2026"), `days_offset`
/// from today (negative = past). ponytail: no chrono dep; std-only date math
/// over days is exact (no DST/calendar edge cases at day granularity here).
fn imap_search_date(days_offset: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let target_days = now / 86_400 + days_offset;
    let (y, m, d) = civil_from_days(target_days);
    format!("{d:02}-{}-{y:04}", MONTHS[(m - 1) as usize])
}

/// Howard Hinnant's days-from-civil algorithm, inverted: days-since-epoch -> (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn parse_full(uid: u32, seen: bool, raw: &[u8]) -> FetchedMessage {
    let mut m = parse_common(uid, seen, raw, "full");
    if let Some(parsed) = MessageParser::default().parse(raw) {
        m.snippet = parsed.body_preview(200).map(|c| c.into_owned());
        m.body = parsed
            .body_html(0)
            .or_else(|| parsed.body_text(0))
            .map(|c| c.into_owned());
        m.attachments = parsed
            .attachments()
            .map(|a| AttachmentMeta {
                filename: a.attachment_name().unwrap_or("attachment").to_string(),
                mime: a
                    .content_type()
                    .map(|ct| match ct.subtype() {
                        Some(sub) => format!("{}/{}", ct.ctype(), sub),
                        None => ct.ctype().to_string(),
                    })
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                size: a.len(),
            })
            .collect();
    }
    m
}

fn parse_meta(uid: u32, seen: bool, raw_headers: &[u8]) -> FetchedMessage {
    parse_common(uid, seen, raw_headers, "meta_only")
}

/// Shared header extraction for both full and metadata-only rows.
fn parse_common(uid: u32, seen: bool, raw: &[u8], mirror_state: &'static str) -> FetchedMessage {
    let parsed = MessageParser::default().parse_headers(raw);
    let (subject, from_name, from_addr, message_id, in_reply_to, references, received_at, headers_raw) =
        match parsed {
            Some(p) => {
                let from = p.from().and_then(|a| a.first());
                let refs = match p.references() {
                    HeaderValue::Text(t) => vec![t.to_string()],
                    HeaderValue::TextList(list) => list.iter().map(|s| s.to_string()).collect(),
                    _ => Vec::new(),
                };
                let in_reply_to = match p.in_reply_to() {
                    HeaderValue::Text(t) => Some(t.to_string()),
                    HeaderValue::TextList(list) => list.first().map(|s| s.to_string()),
                    _ => None,
                };
                (
                    p.subject().map(str::to_string),
                    from.and_then(|a| a.name()).map(str::to_string),
                    from.and_then(|a| a.address()).map(str::to_string),
                    p.message_id().map(str::to_string),
                    in_reply_to,
                    refs,
                    p.date().map(|d| d.to_rfc3339()),
                    String::from_utf8_lossy(raw).into_owned(),
                )
            }
            None => (None, None, None, None, None, Vec::new(), None, String::from_utf8_lossy(raw).into_owned()),
        };

    FetchedMessage {
        uid,
        message_id,
        in_reply_to,
        references,
        subject,
        from_name,
        from_addr,
        received_at,
        seen,
        headers_raw,
        body: None,
        snippet: None,
        attachments: Vec::new(),
        mirror_state,
    }
}

/// Thread root: first Message-ID in the References chain, else In-Reply-To,
/// else the message's own Message-ID. A string key, not a FK — grouping is
/// sync-order-independent (a reply synced before its parent still groups).
/// ponytail: References-root threading, no subject fallback; upgrade to JWZ
/// if real threads fracture.
pub fn thread_id_for(m: &FetchedMessage) -> String {
    m.references
        .first()
        .cloned()
        .or_else(|| m.in_reply_to.clone())
        .or_else(|| m.message_id.clone())
        .unwrap_or_else(|| format!("uid:{}", m.uid))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &[u8] = b"Message-ID: <root@x.example>\r\n\
Subject: =?UTF-8?Q?R=C3=A9union?=\r\n\
From: Marie Dupont <marie@x.example>\r\n\
Date: Mon, 1 Jun 2026 10:00:00 +0000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Bonjour, a tester.\r\n";

    const REPLY: &[u8] = b"Message-ID: <reply@x.example>\r\n\
In-Reply-To: <root@x.example>\r\n\
References: <root@x.example>\r\n\
Subject: Re: =?UTF-8?Q?R=C3=A9union?=\r\n\
From: Marie Dupont <marie@x.example>\r\n\
Date: Mon, 1 Jun 2026 11:00:00 +0000\r\n\
Content-Type: multipart/alternative; boundary=\"b\"\r\n\
\r\n\
--b\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
plain part\r\n\
--b\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>html part</p>\r\n\
--b--\r\n";

    #[test]
    fn root_and_reply_share_a_thread_id() {
        let root = parse_full(1, true, ROOT);
        let reply = parse_full(2, true, REPLY);

        assert_eq!(root.message_id.as_deref(), Some("root@x.example"));
        assert_eq!(thread_id_for(&reply), thread_id_for(&root));
        assert_eq!(thread_id_for(&root), "root@x.example");
    }

    #[test]
    fn rfc2047_subject_is_decoded() {
        let root = parse_full(1, true, ROOT);
        assert_eq!(root.subject.as_deref(), Some("Réunion"));
        assert_eq!(root.from_name.as_deref(), Some("Marie Dupont"));
        assert_eq!(root.from_addr.as_deref(), Some("marie@x.example"));
    }

    #[test]
    fn multipart_body_prefers_html() {
        let reply = parse_full(2, true, REPLY);
        assert_eq!(reply.body.as_deref(), Some("<p>html part</p>"));
    }

    #[test]
    fn meta_only_row_has_no_body() {
        let meta = parse_meta(1, false, ROOT);
        assert_eq!(meta.mirror_state, "meta_only");
        assert!(meta.body.is_none());
        assert_eq!(meta.subject.as_deref(), Some("Réunion"));
    }
}
