import { invoke } from "@tauri-apps/api/core";
import type { Account, AccountConfig, ValidationOutcome } from "./types";

// appPassword is passed per-call and never stored on the JS side.
// Tauri maps snake_case Rust args to camelCase keys here.
export const validateAccount = (config: AccountConfig, appPassword: string) =>
  invoke<ValidationOutcome>("validate_account", { config, appPassword });

export const addAccount = (config: AccountConfig, appPassword: string) =>
  invoke<number>("add_account", { config, appPassword });

export const listAccounts = () => invoke<Account[]>("list_accounts");
