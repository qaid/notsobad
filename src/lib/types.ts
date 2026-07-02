// Mirrors the Rust types in src-tauri/src/connection and db::accounts.

export type AccountConfig = {
  display_name: string;
  imap_host: string;
  imap_port: number;
  smtp_host: string;
  smtp_port: number;
  username: string;
};

export type ProtocolResult = { ok: boolean; error: string | null };
export type ValidationOutcome = { imap: ProtocolResult; smtp: ProtocolResult };

export type Account = {
  id: number;
  display_name: string;
  username: string;
  imap_host: string;
  smtp_host: string;
};

// Mirrors db::folders::Folder. Populated by sync_account's folder discovery.
// `selected` drives opt-in sync (#14 rework): only selected folders are
// synced by sync_account, toggled via set_folder_selected.
export type Folder = {
  id: number;
  account_id: number;
  name: string;
  selected: boolean;
};

// Mirrors db::messages::{MessageSummary, MessageDetail}.
export type MessageSummary = {
  id: number;
  account_id: number;
  thread_id: string;
  from_name: string | null;
  from_addr: string | null;
  subject: string | null;
  snippet: string | null;
  received_at: string | null;
  seen: boolean;
};

export type MessageDetail = {
  id: number;
  account_id: number;
  from_name: string | null;
  from_addr: string | null;
  subject: string | null;
  headers: string;
  body: string | null;
  body_is_html: boolean;
  received_at: string | null;
  seen: boolean;
  mirror_state: "full" | "meta_only";
  uid: number;
};

// Mirrors commands::TranslationResult (issue #5). `translated` is null
// whenever no real translation happened: either the body was detected
// already-English, or the model isn't pulled (pull_hint set in that case).
// Render `original` (with the message's own body_is_html) whenever
// `translated` is null — do NOT treat null as "same as original" and render
// `original` through the plain-text translated-view path; for HTML mail that
// shows raw markup instead of the formatted message (see ThreadReader.svelte).
export type TranslationResult = {
  translated: string | null;
  original: string;
  model: string;
  was_cached: boolean;
  pull_hint: string | null;
};
