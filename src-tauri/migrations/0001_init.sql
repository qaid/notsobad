-- Initial schema for notsobad. Local-only store: mail mirror + AI results + labels.
-- Credentials are NEVER stored here; they live in the macOS Keychain (CLAUDE.md).

CREATE TABLE accounts (
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL DEFAULT 'imap_smtp',  -- room for 'gmail'/'exchange' later (#7)
    display_name TEXT    NOT NULL,
    imap_host    TEXT    NOT NULL,
    imap_port    INTEGER NOT NULL,
    smtp_host    TEXT    NOT NULL,
    smtp_port    INTEGER NOT NULL,
    username     TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
    -- no secret column: app-password lives in the Keychain, keyed by account id
);

-- Mirrored mail. body is NULL for metadata-only rows (older than the mirror window).
-- Schema only this issue; sync logic lands in #3.
CREATE TABLE messages (
    id           INTEGER PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES accounts(id),
    uid          INTEGER,            -- IMAP UID within the account/mailbox
    message_id   TEXT,               -- RFC822 Message-ID header
    headers      TEXT    NOT NULL,   -- raw/serialized headers
    body         TEXT,               -- NULL = metadata-only, fetch on demand
    mirror_state TEXT    NOT NULL DEFAULT 'meta_only',  -- 'full' | 'meta_only'
    received_at  TEXT,
    UNIQUE (account_id, uid)
);

-- AI output keyed to a message so work persists and never re-runs (PRD).
-- Schema only this issue; the assistant lands in #4+.
CREATE TABLE ai_results (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id),
    kind       TEXT    NOT NULL,   -- 'translation' | 'summary' | 'triage'
    result     TEXT    NOT NULL,
    model      TEXT    NOT NULL,
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (message_id, kind)
);

-- Local-only, presentation-only labels. NEVER written to the mail server (ADR 0003).
-- Schema only this issue; triage lands in #5.
CREATE TABLE labels (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id),
    name       TEXT    NOT NULL,   -- e.g. 'urgent' | 'needs_reply' | 'fyi' | 'newsletter_promo'
    source     TEXT    NOT NULL DEFAULT 'triage',  -- 'triage' | 'user'
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);
