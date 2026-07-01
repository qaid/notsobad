-- Read path (#3): display columns, unread state, threading inputs, and
-- per-account UID sync bookkeeping. body/mirror_state already exist (#2).

ALTER TABLE messages ADD COLUMN subject TEXT;
ALTER TABLE messages ADD COLUMN from_addr TEXT;
ALTER TABLE messages ADD COLUMN from_name TEXT;
ALTER TABLE messages ADD COLUMN seen INTEGER NOT NULL DEFAULT 0;       -- server \Seen flag, read-only mirror
ALTER TABLE messages ADD COLUMN in_reply_to TEXT;
ALTER TABLE messages ADD COLUMN refs TEXT;                             -- "references" is a SQL reserved word
ALTER TABLE messages ADD COLUMN thread_id TEXT;                        -- root Message-ID of the References chain
ALTER TABLE messages ADD COLUMN snippet TEXT;                          -- short body preview, full-mirror rows only
ALTER TABLE messages ADD COLUMN attachments TEXT;                      -- nullable JSON index: [{filename,mime,size}]

CREATE INDEX idx_messages_thread ON messages(account_id, thread_id);

-- UID sync bookkeeping so incremental sync can resume and detect server resets.
ALTER TABLE accounts ADD COLUMN uidvalidity INTEGER;
ALTER TABLE accounts ADD COLUMN last_uid INTEGER NOT NULL DEFAULT 0;
