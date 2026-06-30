# 0003. Triage labels are local-only; never mutate the mail server

Date: 2026-06-30
Status: Accepted

## Context

Triage auto-categorizes incoming mail (Urgent / Needs-reply / FYI / Newsletter-promo)
and the user wanted low-value mail "auto-filed" out of the main inbox. Auto-filing by
moving mail on the server is risky: a local LLM will sometimes misclassify, and a move
or delete on the real mailbox is hard to undo and could hide important mail across all
the user's clients.

## Decision

Triage applies **local labels** stored in the app's SQLite DB, keyed to message ID.
Labels drive a **presentation/visibility layer only** (e.g. a "Filtered" view hides
Newsletter-promo from the main inbox). The app **never** moves, deletes, or writes flags
on the mail server. "Auto-file" means "hide from a view," not "move on the server."

For v1, labels are **local-only** — they do not sync back as Gmail labels or IMAP keywords.
Optional server-side label sync is deferred to v2.

## Consequences

- The app cannot damage or reorganize the real mailbox; the server stays pristine.
- Misclassification is always reversible with a local relabel; no server round-trip.
- Labels are not visible in other mail clients (acceptable for a single-user local app).
- Filed mail is never lost — it is one click away in a Filtered view, with a count badge.
- A confidence gate and one-click "wrong label" correction (with sender-level override)
  improve trust over time.
