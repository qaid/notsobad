# PRD: notsobad — a local-first, AI-assisted email client

Status: ready-for-agent
Date: 2026-06-30
Companion docs: `CONTEXT.md` (glossary), `docs/adr/` (decisions), `docs/research/local-llm-models.md`

## Problem Statement

The owner runs many email accounts across multiple providers and gets a lot of mail in
languages other than English (Dutch and French most often). Reading foreign mail means
manually translating it. The inbox is noisy: newsletters and low-value mail bury the
things that need attention. Existing clients either lack good built-in AI or send mail
content to the cloud, which the owner does not want. The owner wants one client that reads
and sends mail, runs entirely on their own machine, and uses local AI to remove the daily
friction — without ever sending mail content to a cloud service or altering the real
mailboxes on the servers.

## Solution

A native macOS desktop email **client** with an **assistant** (local LLM) woven in. It
connects to the owner's accounts, mirrors recent mail locally, and uses local Ollama models
to translate foreign mail inline, triage the inbox, summarize long threads, and draft
replies. All AI runs locally; no mail content leaves the machine. The app never mutates the
mail servers — all categorization is expressed as **local labels** that change presentation
only.

## User Stories

1. As the owner, I want to add an email account once inside the app, so that I do not have to re-enter credentials every session.
2. As the owner, I want to connect a generic IMAP/SMTP account, so that any provider works.
3. As the owner, I want to connect a Gmail account via OAuth, so that I get Gmail's richer features.
4. As the owner, I want to connect an Exchange Online account, so that my Microsoft 365 mail is included.
5. As the owner, I want my credentials stored securely in the system Keychain, so that they are not sitting in plaintext.
6. As the owner, I want recent mail (last 6 months) mirrored locally, so that reading is fast and works offline.
7. As the owner, I want older mail available as metadata with bodies fetched on demand, so that multi-year mailboxes do not bloat local storage.
8. As the owner, I want to see a unified inbox list, so that I can scan what arrived.
9. As the owner, I want to open a thread and read the full conversation, so that I have context.
10. As the owner, I want foreign-language mail shown already translated to English, so that I never have to translate it myself.
11. As the owner, I want the app to auto-detect the language, so that translation works without me configuring per-language.
12. As the owner, I want the English translation shown by default with a toggle to reveal the original, so that I can check the source when needed.
13. As the owner, I want translations cached, so that re-opening a message is instant.
14. As the owner, I want incoming mail auto-classified into Urgent / Needs-reply / FYI / Newsletter-promo, so that I see what matters first.
15. As the owner, I want Newsletter-promo mail hidden from the main inbox into a Filtered view, so that noise is out of the way.
16. As the owner, I want triage to apply local labels only and never move or delete mail on the server, so that my real mailboxes are never altered.
17. As the owner, I want to correct a wrong label in one click and have the app remember it for that sender, so that mistakes do not repeat.
18. As the owner, I want a TL;DR summary of long threads, so that I can catch up quickly.
19. As the owner, I want the app to draft a reply in my voice, so that I can edit and send instead of writing from scratch.
20. As the owner, I want to compose mail in Markdown, so that formatting is fast to write.
21. As the owner, I want my Markdown rendered to HTML on send, so that the recipient sees a formatted email and never raw Markdown.
22. As the owner, I want reply, reply-all, and forward with threading preserved, so that conversations stay intact.
23. As the owner, I want to attach files to outgoing mail, so that I can send documents.
24. As the owner, I want to choose which local model handles each AI task, so that I can swap in better models over time.
25. As the owner, I want the app to tell me if a required model is not installed, so that I know what to pull.
26. As the owner, I want all AI to run locally via Ollama, so that no mail content is sent to the cloud.

## Implementation Decisions

- **Platform**: native desktop app built with **Tauri 2.x** — Rust backend + web UI. (ADR 0002)
- **UI framework**: **Svelte / SvelteKit**. (ADR 0002 context)
- **Local store**: a single **SQLite** database. Mail (headers/bodies/attachment index) and all AI results (translations, summaries, triage labels) live here, keyed to message ID, so AI work persists and never re-runs.
- **Mirror policy**: full mirror for mail from the **last 6 months**; metadata-only with on-demand body fetch + recently-read cache for older mail. Same window across all accounts.
- **Connection layer**: one abstraction with three backends — generic **IMAP/SMTP** (built and hardened first), **Gmail API** (OAuth), **Exchange Online** (Graph or OAuth-IMAP). On-prem Exchange/EWS is out of scope. (ADR 0001)
- **Credentials**: stored in the macOS **Keychain** via the Rust backend. No reuse of system mail accounts (OS does not expose them — ADR 0001).
- **Labels are local-only and presentation-only**: triage writes labels to SQLite; the app **never** moves, deletes, or sets server flags. "Auto-file" = hide in a Filtered view. (ADR 0003)
- **Triage taxonomy**: fixed buckets Urgent / Needs-reply / FYI / Newsletter-promo. A confidence gate keeps low-confidence mail in the inbox; one-click correction creates a sender-level override.
- **AI timing (hybrid)**: triage runs eagerly on arrival (cheap, fast model); translate + summarize run on-open (heavier, only for mail actually read); draft runs on request.
- **Model mapping** (configurable per task, not hardcoded):
  - Translate (NL/FR→EN): `alibayram/erurollm-9b-instruct` (EuroLLM-9B-Instruct, community tag, ~5.6GB; verify pullable at build). Fallback `gemma4:e4b`.
  - Triage (fast): `qwen2.5:3b`.
  - Summarize / Draft: `qwen3.5:9b`.
- **Compose rendering**: Markdown → HTML email with a plain-text alternative part. Recipient never sees raw Markdown.
- **AI transport**: local Ollama HTTP API from the Rust backend. No cloud LLM calls for any core feature.

## Testing Decisions

- **Seam 1 — the connection/sync layer** (Rust): test against a mailbox backend behind the IMAP/SMTP abstraction. Test external behavior: "given a mailbox with messages, sync produces the expected local rows," "send produces a well-formed MIME message." Use a local/mock IMAP+SMTP server rather than a live account.
- **Seam 2 — the assistant layer** (Rust): test that each task sends the right prompt to Ollama and stores the result keyed to message ID, with a fake Ollama endpoint. Assert behavior (translation cached, triage label written locally) not prompt wording.
- **Guardrail test (highest value)**: assert the app issues **zero** server-mutating commands (no IMAP STORE/MOVE/EXPUNGE, no Gmail modify) during triage/labeling. This protects ADR 0003 directly.
- **Compose test**: Markdown input → HTML output contains expected formatting and a text/plain alternative part; no raw Markdown leaks into the sent message.
- Prefer the fewest seams; test external behavior, not implementation details.

## Out of Scope (v1)

- Smart search / "ask your inbox" (RAG over mail; needs embeddings + vector store) — v2.
- Follow-up tracking, unsubscribe/cleanup — v2.
- Outbound translation (compose in English, send in recipient's language) — v2.
- Server-side label sync (pushing labels as Gmail labels / IMAP keywords) — v2.
- On-prem Exchange (EWS/NTLM).
- Multi-user / multi-tenant / any auth for people other than the owner.
- Rich-text WYSIWYG editing beyond Markdown (tables, inline images).
- Custom user-defined triage categories — v2 (fixed buckets in v1).

## Further Notes

- Build order (each step independently usable): (1) Tauri+Svelte shell + SQLite + connection wizard; (2) read path (sync + list + reader); (3) send path (Markdown compose); (4) translate; (5) triage; (6) summarize + draft; (7) multi-account (Gmail API, Exchange Online). These map to the seven GitHub issues filed alongside this PRD.
- Hardware target: MacBook Pro M1 Max, 32GB. ~21GB usable for models; comfortably runs a small triage model + a mid translate/summarize model concurrently.
- The translation model is a community Ollama upload, not an official-library model; keep the official `gemma4:e4b` as a guaranteed fallback and verify the EuroLLM pull during the translate step.
