import type { Account, MessageDetail, MessageSummary } from "./types";
import { listAccounts, listInbox, syncAccount, threadMessages } from "./ipc";

// ponytail: a module-level rune object is the whole store. No external state lib
// for one list + one boolean.
export const app = $state({
  accounts: [] as Account[],
  showWizard: false,
  inbox: [] as MessageSummary[],
  currentThread: null as MessageDetail[] | null,
});

export async function refreshAccounts() {
  app.accounts = await listAccounts();
}

export async function refreshInbox() {
  app.inbox = await listInbox();
}

export async function syncAndRefresh(accountId: number) {
  await syncAccount(accountId);
  await refreshInbox();
}

export async function openThread(threadId: string) {
  app.currentThread = await threadMessages(threadId);
}

export function closeThread() {
  app.currentThread = null;
}
