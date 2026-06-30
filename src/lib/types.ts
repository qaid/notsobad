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
