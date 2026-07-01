import type { Account, Folder, MessageDetail, MessageSummary } from "./types";
import {
  listAccounts,
  listFolderMessages,
  listFolders,
  listInbox,
  setFolderSelected,
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
  // null = the unified inbox spanning every account's INBOX. This is the
  // default on load, AND a genuinely reachable state via selectUnifiedInbox()
  // (wired to the sidebar's "All Inboxes" row) — not just an initial value
  // that becomes permanently unreachable once a folder is clicked.
  //
  // selectFolder(accountId, name) always sets {accountId, name}, including
  // for INBOX — no "name === INBOX -> null" sentinel here. That sentinel was
  // the sidebar-highlight bug: it conflated "this account's INBOX" with
  // "everyone's INBOX," so clicking one account's INBOX highlighted every
  // account's INBOX row. Fix: {accountId, name} distinguishes accounts;
  // selectUnifiedInbox() is the one explicit path back to null.
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
  // Always {accountId, name}, including for INBOX — no null sentinel here.
  // See the `currentFolder` doc comment: a bare "INBOX -> null" sentinel
  // can't distinguish which account's INBOX is selected.
  app.currentFolder = { accountId, name };
  await refreshInbox();
}

// The one explicit path back to the unified cross-account inbox (#3's "All
// Inboxes" view). Wired to the sidebar's "All Inboxes" row.
export async function selectUnifiedInbox() {
  app.currentFolder = null;
  await refreshInbox();
}

export async function syncAndRefresh(accountId: number) {
  await syncAccount(accountId);
  await refreshFolders(accountId);
  await refreshInbox();
}

// Toggle a folder's opt-in sync selection, then trigger a full sync so a
// newly-selected folder is populated immediately rather than waiting for the
// next manual sync. set_folder_selected itself is a pure SQLite write (no
// IMAP); syncAndRefresh is the same already-guardrailed sync path every
// other sync goes through.
export async function toggleFolderSelected(accountId: number, name: string, selected: boolean) {
  await setFolderSelected(accountId, name, selected);
  await refreshFolders(accountId);
  if (selected) {
    await syncAndRefresh(accountId);
  }
}

export async function openThread(accountId: number, threadId: string) {
  app.currentThread = await threadMessages(accountId, threadId);
}

export function closeThread() {
  app.currentThread = null;
}
