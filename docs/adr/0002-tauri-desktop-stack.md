# 0002. Tauri 2.x as the desktop framework

Date: 2026-06-30
Status: Accepted

## Context

The app is a native-feeling Mac desktop email client, local-only, single-user,
talking to IMAP/SMTP, Gmail API, Exchange Online, the system Keychain, and a local
Ollama server. We wanted the lightest framework that is still fully capable, and
checked for anything newer than Tauri.

## Decision

Use **Tauri 2.x**: Rust backend + web UI using the OS native WebView.

Alternatives considered:
- **Electron**: heavier (≈10x larger, ≈5x more RAM); rejected for weight.
- **Wails** (Go backend): comparable, but Rust's ecosystem is stronger for mail
  protocols and native integration.
- **Neutralino**: too limited for a full mail client.

## Consequences

- Small installer, low memory, scoped native permissions (Tauri v2 security model).
- Rust backend handles IMAP/SMTP, OAuth token storage, Keychain, and Ollama HTTP calls.
- Web UI (framework TBD in a later decision) is fast to build and iterate.
- Team must be comfortable with some Rust for the backend commands.
