import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  AccountConfig,
  Folder,
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

// Sync every folder on the account (full mirror for the last 6 months per
// folder, metadata-only further back). Read-only against the server (ADR 0003).
export const syncAccount = (accountId: number) =>
  invoke<number>("sync_account", { accountId });

// This account's tracked folders, discovered by the last sync_account call.
export const listFolders = (accountId: number) =>
  invoke<Folder[]>("list_folders", { accountId });

// Inbox list: one row per thread, newest first, INBOX only. Pass null/undefined for accountId to span all accounts.
export const listInbox = (accountId?: number) =>
  invoke<MessageSummary[]>("list_inbox", { accountId: accountId ?? null });

// One named folder's message list (one row per thread, newest first).
export const listFolderMessages = (folderName: string, accountId?: number) =>
  invoke<MessageSummary[]>("list_folder_messages", { accountId: accountId ?? null, folderName });

export const threadMessages = (accountId: number, threadId: string) =>
  invoke<MessageDetail[]>("thread_messages", { accountId, threadId });

// Lazily fetches the body from the server if this is a metadata-only message.
// body_is_html is the backend's own parser signal, not a frontend guess.
export const messageBody = (messageId: number) =>
  invoke<{ body: string; body_is_html: boolean }>("message_body", { messageId });
