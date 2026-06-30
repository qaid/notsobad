import type { Account } from "./types";
import { listAccounts } from "./ipc";

// ponytail: a module-level rune object is the whole store. No external state lib
// for one list + one boolean.
export const app = $state({
  accounts: [] as Account[],
  showWizard: false,
});

export async function refreshAccounts() {
  app.accounts = await listAccounts();
}
