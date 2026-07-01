<script lang="ts">
  import { addAccount, validateAccount } from "../ipc";
  import type { AccountConfig, ValidationOutcome } from "../types";
  import { app, refreshAccounts } from "../store.svelte";

  // ponytail: plain bound fields, no form library. Ports default to IMAPS/submission.
  let config = $state<AccountConfig>({
    display_name: "",
    imap_host: "",
    imap_port: 993,
    smtp_host: "",
    smtp_port: 587,
    username: "",
  });
  let appPassword = $state("");

  let busyAction = $state<"validate" | "save" | null>(null);
  let result = $state<ValidationOutcome | null>(null);
  let error = $state<string | null>(null);

  async function validate() {
    busyAction = "validate";
    error = null;
    result = null;
    try {
      result = await validateAccount($state.snapshot(config), appPassword);
    } catch (e) {
      error = String(e);
    } finally {
      busyAction = null;
    }
  }

  async function save() {
    busyAction = "save";
    error = null;
    try {
      await addAccount($state.snapshot(config), appPassword);
      appPassword = "";
      await refreshAccounts();
      app.showWizard = false;
    } catch (e) {
      error = String(e);
    } finally {
      busyAction = null;
    }
  }
</script>

<section class="wizard">
  <h2>Add IMAP/SMTP account</h2>

  <label>Display name<input bind:value={config.display_name} /></label>
  <label>Username (email)<input bind:value={config.username} autocomplete="off" /></label>
  <label>App password<input type="password" bind:value={appPassword} autocomplete="off" /></label>

  <div class="row">
    <label>IMAP host<input bind:value={config.imap_host} /></label>
    <label class="port">Port<input type="number" bind:value={config.imap_port} /></label>
  </div>
  <div class="row">
    <label>SMTP host<input bind:value={config.smtp_host} /></label>
    <label class="port">Port<input type="number" bind:value={config.smtp_port} /></label>
  </div>

  <div class="actions">
    <button onclick={validate} disabled={busyAction !== null}>Validate</button>
    <button
      onclick={save}
      disabled={busyAction !== null || !result?.imap.ok || !result?.smtp.ok}
    >
      Save
    </button>
    <button class="ghost" onclick={() => (app.showWizard = false)} disabled={busyAction !== null}>
      Cancel
    </button>
  </div>

  {#if busyAction}
    <p class="status-busy">
      <span class="spinner"></span>
      {busyAction === "validate" ? "Checking credentials…" : "Saving account…"}
    </p>
  {/if}
  {#if error}<p class="err">{error}</p>{/if}
  {#if result}
    <ul class="status">
      <li class={result.imap.ok ? "ok" : "fail"}>
        IMAP: {result.imap.ok ? "connected" : result.imap.error}
      </li>
      <li class={result.smtp.ok ? "ok" : "fail"}>
        SMTP: {result.smtp.ok ? "connected" : result.smtp.error}
      </li>
    </ul>
  {/if}
</section>

<style>
  .wizard {
    max-width: 480px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    font-size: 0.85em;
    gap: 3px;
  }
  input {
    padding: 6px;
  }
  .row {
    display: flex;
    gap: 10px;
  }
  .row label {
    flex: 1;
  }
  .port {
    flex: 0 0 90px !important;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  button {
    padding: 8px 14px;
    cursor: pointer;
  }
  .ghost {
    margin-left: auto;
  }
  .status {
    list-style: none;
    padding: 0;
  }
  .ok {
    color: green;
  }
  .fail,
  .err {
    color: #c00;
  }
  .status-busy {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border, #ddd);
    border-top-color: var(--fg, #111);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
