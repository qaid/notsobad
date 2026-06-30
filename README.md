# notsobad

A local-first, AI-assisted email client for macOS. Single user, runs entirely on your own
machine. Reads and sends mail across multiple accounts, and uses **local** LLMs (via Ollama)
to remove daily friction — without sending any mail content to the cloud and without ever
altering your mailboxes on the server.

## Why

- Lots of mail in **Dutch and French** that you'd otherwise translate by hand.
- A noisy inbox where newsletters bury what matters.
- A hard requirement that mail content **never leaves the machine** and the real mailboxes
  are **never modified**.

## What it does (v1)

- **Reads and sends mail** over IMAP/SMTP (first), then Gmail API and Exchange Online.
- **Auto-translates** foreign-language mail to English inline (auto-detected; toggle to see
  the original).
- **Triages** the inbox into Urgent / Needs-reply / FYI / Newsletter-promo and hides noise —
  using **local labels only**, never touching the server.
- **Summarizes** long threads and **drafts replies** in your voice.
- **Composes in Markdown**, sends as formatted HTML.

## Stack

- **Tauri 2.x** (Rust backend + web UI), **Svelte/SvelteKit** frontend, **SQLite** local store.
- **Ollama** for all AI. No cloud LLM, ever.
- Target hardware: Apple Silicon Mac (developed against M1 Max, 32GB).

## Core guarantee

The app **never mutates the mail server** — no moves, deletes, or server-side flags. All
categorization is local presentation only. See `docs/adr/0003-local-labels-no-server-mutation.md`.

## Project docs

- `docs/PRD.md` — full product requirements.
- `CONTEXT.md` — glossary and key decisions.
- `docs/adr/` — architecture decision records.
- `docs/research/local-llm-models.md` — model selection research.

Built by Claude Code. The v1 build is tracked as seven sequenced GitHub issues labeled
`ready-for-agent`.
