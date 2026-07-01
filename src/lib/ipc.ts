import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  AccountConfig,
  MessageDetail,
  MessageSummary,
  ValidationOutcome,
} from "./types";

// appPassword is passed per-call and never stored on the JS side.
// Tauri maps snake_case Rust args to camelCase keys here.
export const validateAccount = (config: AccountConfig, appPassword: string) =>
  invoke<ValidationOutcome>("validate_account", { config, appPassword });

export const addAccount = (config: AccountConfig, appPassword: string) =>
  invoke<number>("add_account", { config, appPassword });

export const listAccounts = () => invoke<Account[]>("list_accounts");

// Sync one account's INBOX (full mirror for the last 6 months, metadata-only
// further back). Read-only against the server (ADR 0003).
export const syncAccount = (accountId: number) =>
  invoke<number>("sync_account", { accountId });

// Unified inbox: one row per thread, newest first. Pass null/undefined for accountId to span all accounts.
export const listInbox = (accountId?: number) =>
  invoke<MessageSummary[]>("list_inbox", { accountId: accountId ?? null });

export const threadMessages = (accountId: number, threadId: string) =>
  invoke<MessageDetail[]>("thread_messages", { accountId, threadId });

// Lazily fetches the body from the server if this is a metadata-only message.
// body_is_html is the backend's own parser signal, not a frontend guess.
export const messageBody = (messageId: number) =>
  invoke<{ body: string; body_is_html: boolean }>("message_body", { messageId });
