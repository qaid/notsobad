# notsobad

Local-first, AI-assisted email client. Single user, runs entirely on the owner's Mac.
Built by Claude Code. Full spec in `docs/PRD.md`; glossary in `CONTEXT.md`; decisions in `docs/adr/`.

## Stack

- **Tauri 2.x** — Rust backend + web UI.
- **Svelte / SvelteKit** frontend.
- **SQLite** local store (mail + AI results, keyed to message ID).
- **Ollama** for all AI (local HTTP API). No cloud LLM, ever.

## Hard rules (do not violate)

- **Never mutate the mail server.** No IMAP STORE/MOVE/EXPUNGE, no Gmail/Graph modify, no
  deletes, no server-side flags. All categorization is **local labels** in SQLite that affect
  presentation only. This is ADR 0003 and is the core safety guarantee. A change that writes
  to the server is a bug.
- **No mail content leaves the machine.** AI runs locally via Ollama. Do not add cloud LLM calls.
- **Credentials live in the macOS Keychain**, accessed from the Rust backend. Never log or
  persist them in plaintext or in the SQLite DB.

## AI task → model mapping (configurable, not hardcoded)

| Task | Model | Notes |
|---|---|---|
| Translate (NL/FR→EN) | `alibayram/erurollm-9b-instruct` | EuroLLM-9B-Instruct, community tag (~5.6GB). Fallback `gemma4:e4b`. Verify pull before relying on it. |
| Triage (on arrival, fast) | `qwen2.5:3b` | Cheap, runs on every message. |
| Summarize / Draft | `qwen3.5:9b` | |

Pull the translation model: `ollama pull alibayram/erurollm-9b-instruct`
(check `ollama list` first; fall back to `gemma4:e4b` if the pull fails).

## Conventions

- Test external behavior, not implementation. The highest-value test asserts the app issues
  zero server-mutating commands during triage/labeling (protects ADR 0003).
- Build order is the 7 GitHub issues, in sequence. IMAP/SMTP is the first connection backend.
- Keep it minimal — see the project's lazy bias. No speculative abstraction.
