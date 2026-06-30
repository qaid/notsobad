# Context: notsobad (email app)

Glossary and shared language for the project. Devoid of implementation detail.
Spec, scope, and build order live in `docs/PRD.md`; decisions in `docs/adr/`.

## Decisions (one-line; full detail in ADRs / PRD)

- **App category**: hybrid email **client + AI**, single user, runs **locally**.
- **LLM**: **local models via Ollama** only; no cloud LLM at runtime.
- **"Claude project"**: built by Claude Code; Claude is the dev tool, not a runtime dependency.
- **Stack**: **Tauri 2.x** (Rust + web UI), **Svelte/SvelteKit** frontend, **SQLite** store. (ADR 0002)
- **Accounts**: one-time in-app setup; no macOS system-account reuse. IMAP/SMTP + Gmail API + Exchange Online. (ADR 0001)
- **Labels never mutate the server**; categorization is local presentation only. (ADR 0003)

## Glossary

- **Account**: an external email mailbox the app connects to (e.g. a Gmail or IMAP account). Distinct from "user" — there is exactly one user, but possibly several accounts.
- **Client**: the part of the app that reads and sends mail through connected accounts.
- **Assistant**: the AI layer (local LLM via Ollama) that acts on mail — triage, summarize, draft, translate.
- **Label**: a local-only tag applied to a message, stored in the app's SQLite DB. Drives visibility/presentation only. Never written to the mail server (no moves, no server flags). See ADR 0003.
- **Filtered view**: a presentation that hides labeled mail (e.g. Newsletter-promo) from the main inbox. The mail is untouched and one click away.
- **Mirror**: the locally stored copy of mail. Full (headers + bodies) for the last 6 months; metadata-only with on-demand body fetch beyond.
