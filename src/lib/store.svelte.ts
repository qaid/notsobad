import { goto } from "$app/navigation";
import type { Account, Folder, MessageDetail, MessageSummary, TranslationResult } from "./types";
import {
  listAccounts,
  listFolderMessages,
  listFolders,
  listInbox,
  setFolderSelected,
  syncAccount,
  threadMessages,
  translateMessage,
} from "./ipc";

export type ThemePref = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "theme-pref";

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
  // Translate-on-open results (#5), keyed by message id. Additive to the
  // per-thread fetchedBodies pattern in ThreadReader: a message only gets an
  // entry once translate_message has been called for it (on open), so
  // presence (not truthiness) is the "has this been translated" check —
  // same reasoning as fetchedBodies, an all-English body legitimately
  // produces original === translated, which is still a real result.
  translations: {} as Record<number, TranslationResult>,
  // "Show original" toggle state per message, default false (English shown).
  showOriginal: {} as Record<number, boolean>,
});

// Separate from `app`: this is a display preference, not app data, and its
// initial value is read from localStorage rather than an IPC call.
export const theme = $state({
  pref: (localStorage.getItem(THEME_STORAGE_KEY) as ThemePref | null) || "system",
});

export function setThemePref(pref: ThemePref) {
  theme.pref = pref;
  localStorage.setItem(THEME_STORAGE_KEY, pref);
  applyTheme();
}

// Resolves "system" via prefers-color-scheme and writes the result to
// data-theme on <html>, which is what +layout.svelte's CSS vars key off of.
export function applyTheme() {
  const resolved =
    theme.pref === "system"
      ? matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme.pref;
  document.documentElement.setAttribute("data-theme", resolved);
}

// Plain (non-reactive) set of message ids with a translate_message call
// currently in flight. ThreadReader's on-open $effect re-runs on every
// app.currentThread/app.translations write, so guarding only on `messageId in
// app.translations` isn't enough — every re-run before the first call
// resolves would fire another ~seconds-long Ollama request for the same
// message. A plain Set (not $state) is checked+added synchronously before
// the await, so a same-tick re-entrant call is blocked immediately rather
// than racing the first call's still-pending promise.
const translateInFlight = new Set<number>();

// Translate a message's body to English and cache the result in the store,
// keyed by message id. No-op if already present in this session's cache, or
// already in flight — never cleared, since the underlying ai_results row is
// itself a permanent SQLite cache (re-opening a thread across app restarts is
// still instant via translate_message's own DB-level cache hit). Requires the
// body already loaded (messageBody called first) — translate_message itself
// never talks to the mail server.
export async function translateAndCache(messageId: number) {
  if (messageId in app.translations || translateInFlight.has(messageId)) return;
  translateInFlight.add(messageId);
  try {
    app.translations[messageId] = await translateMessage(messageId);
  } finally {
    translateInFlight.delete(messageId);
  }
}

export function toggleShowOriginal(messageId: number) {
  app.showOriginal[messageId] = !app.showOriginal[messageId];
}

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
  // Close any open thread: +page.svelte gates the content pane on
  // currentThread, so without this a folder click updates app.inbox behind
  // a still-showing ThreadReader and looks like the sidebar is dead until
  // you hit the reader's back button.
  app.currentThread = null;
  // #13 added a real /settings route. This state is only ever rendered by
  // +page.svelte (route "/"), so clicking a folder while on /settings updated
  // state nothing on-screen reads — looked like the sidebar had frozen.
  await goto("/");
  await refreshInbox();
}

// The one explicit path back to the unified cross-account inbox (#3's "All
// Inboxes" view). Wired to the sidebar's "All Inboxes" row.
export async function selectUnifiedInbox() {
  app.currentFolder = null;
  // Same reason as selectFolder: drop out of the thread reader so the
  // unified inbox actually shows when picked.
  app.currentThread = null;
  // See selectFolder's comment: route back to "/" so this is visible from /settings.
  await goto("/");
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
