-- Opt-in folder sync (#14 rework): discovery (LIST) still upserts every
-- folder so the picker can show what's available, but sync only loops over
-- folders the user has explicitly selected. INBOX keeps today's behavior
-- (already syncing); everything else becomes opt-in.
--
-- Existing rows: only INBOX was ever synced before this column existed, so
-- backfill it (and only it) to selected — defaulting every row to selected
-- would silently start syncing Archive/Sent/etc. for existing accounts on
-- upgrade, which is exactly the behavior this migration is opting folders
-- out of.

ALTER TABLE folders ADD COLUMN selected INTEGER NOT NULL DEFAULT 0;

UPDATE folders SET selected = 1 WHERE name = 'INBOX';
