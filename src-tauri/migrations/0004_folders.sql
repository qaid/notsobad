-- Multi-folder sync (#14): folders become first-class, UIDs are unique per
-- folder (not per account) since two folders can both have a UID 42.
--
-- SQLite can't ALTER a NOT NULL REFERENCES column into an existing table, and
-- can't drop the old UNIQUE(account_id, uid) table constraint from 0001 — both
-- require a table rebuild (create-copy-drop-rename), not ALTER TABLE.

CREATE TABLE folders (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts(id),
    name        TEXT    NOT NULL,           -- IMAP folder/mailbox name
    uidvalidity INTEGER,
    last_uid    INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, name)
);

-- Backfill: every existing account gets one INBOX folder row, carrying over
-- its account-level UID bookkeeping so incremental sync doesn't restart.
INSERT INTO folders (account_id, name, uidvalidity, last_uid)
SELECT id, 'INBOX', uidvalidity, last_uid FROM accounts;

-- Rebuild messages with folder_id (NOT NULL REFERENCES) and a per-folder
-- UNIQUE constraint. Existing rows map to their account's INBOX folder.
CREATE TABLE messages_new (
    id           INTEGER PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES accounts(id),
    folder_id    INTEGER NOT NULL REFERENCES folders(id),
    uid          INTEGER,
    message_id   TEXT,
    headers      TEXT    NOT NULL,
    body         TEXT,
    body_is_html INTEGER NOT NULL DEFAULT 0,
    mirror_state TEXT    NOT NULL DEFAULT 'meta_only',
    received_at  TEXT,
    subject      TEXT,
    from_addr    TEXT,
    from_name    TEXT,
    seen         INTEGER NOT NULL DEFAULT 0,
    in_reply_to  TEXT,
    refs         TEXT,
    thread_id    TEXT,
    snippet      TEXT,
    attachments  TEXT,
    UNIQUE (folder_id, uid)
);

INSERT INTO messages_new
    (id, account_id, folder_id, uid, message_id, headers, body, body_is_html,
     mirror_state, received_at, subject, from_addr, from_name, seen,
     in_reply_to, refs, thread_id, snippet, attachments)
SELECT m.id, m.account_id, f.id, m.uid, m.message_id, m.headers, m.body, m.body_is_html,
       m.mirror_state, m.received_at, m.subject, m.from_addr, m.from_name, m.seen,
       m.in_reply_to, m.refs, m.thread_id, m.snippet, m.attachments
FROM messages m
JOIN folders f ON f.account_id = m.account_id AND f.name = 'INBOX';

DROP TABLE messages;
ALTER TABLE messages_new RENAME TO messages;

CREATE INDEX idx_messages_thread ON messages(account_id, thread_id);
CREATE INDEX idx_messages_folder ON messages(folder_id);

-- accounts.uidvalidity/last_uid are superseded by per-folder state in
-- `folders` but left in place (dropping needs another rebuild); sync no
-- longer reads or writes them.
