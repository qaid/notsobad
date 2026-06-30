import { invoke } from "@tauri-apps/api/core";
import type { Account, AccountConfig, ValidationOutcome } from "./types";

// app_password is passed per-call and never stored on the JS side.
export const validateAccount = (config: AccountConfig, app_password: string) =>
  invoke<ValidationOutcome>("validate_account", { config, app_password });

export const addAccount = (config: AccountConfig, app_password: string) =>
  invoke<number>("add_account", { config, app_password });

export const listAccounts = () => invoke<Account[]>("list_accounts");
