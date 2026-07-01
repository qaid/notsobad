import type { Account, Folder, MessageDetail, MessageSummary } from "./types";
import {
  listAccounts,
  listFolderMessages,
  listFolders,
  listInbox,
  syncAccount,
  threadMessages,
} from "./ipc";

// ponytail: a module-level rune object is the whole store. No external state lib
// for one list + one boolean.
export const app = $state({
  accounts: [] as Account[],
  showWizard: false,
  inbox: [] as MessageSummary[],
  currentThread: null as MessageDetail[] | null,
  // Per-account folder lists, keyed by account id (#14). INBOX is the default
  // view; other folders are reached by explicitly selecting one.
  folders: {} as Record<number, Folder[]>,
  // null = INBOX (the default, unified across accounts). Set to view one
  // account's non-INBOX folder instead.
  currentFolder: null as { accountId: number; name: string } | null,
});

export async function refreshAccounts() {
  app.accounts = await listAccounts();
  // Folders are persisted (populated by a prior sync_account call), so load
  // them alongside accounts on every refresh — otherwise the sidebar's folder
  // switcher would show nothing until the user clicks Sync again, even though
  // the rows already exist in SQLite from a previous session.
  await Promise.all(app.accounts.map((a) => refreshFolders(a.id)));
}

export async function refreshFolders(accountId: number) {
  app.folders[accountId] = await listFolders(accountId);
}

export async function refreshInbox() {
  if (app.currentFolder) {
    app.inbox = await listFolderMessages(app.currentFolder.name, app.currentFolder.accountId);
  } else {
    app.inbox = await listInbox();
  }
}

export async function selectFolder(accountId: number, name: string) {
  app.currentFolder = name === "INBOX" ? null : { accountId, name };
  await refreshInbox();
}

export async function syncAndRefresh(accountId: number) {
  await syncAccount(accountId);
  await refreshFolders(accountId);
  await refreshInbox();
}

export async function openThread(accountId: number, threadId: string) {
  app.currentThread = await threadMessages(accountId, threadId);
}

export function closeThread() {
  app.currentThread = null;
}
