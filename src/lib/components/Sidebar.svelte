<script lang="ts">
  import { app, selectFolder, syncAndRefresh } from "../store.svelte";

  let syncing = $state<number | null>(null);

  async function sync(accountId: number) {
    syncing = accountId;
    try {
      await syncAndRefresh(accountId);
    } finally {
      syncing = null;
    }
  }

  function isCurrent(accountId: number, name: string) {
    return name === "INBOX"
      ? app.currentFolder === null
      : app.currentFolder?.accountId === accountId && app.currentFolder?.name === name;
  }
</script>

<aside class="sidebar">
  <button class="add" onclick={() => (app.showWizard = true)}>+ Add account</button>
  <ul>
    {#each app.accounts as acct (acct.id)}
      <li>
        <div class="row">
          <div>
            <div class="name">{acct.display_name}</div>
            <div class="sub">{acct.username}</div>
          </div>
          <button class="sync" disabled={syncing === acct.id} onclick={() => sync(acct.id)}>
            {syncing === acct.id ? "…" : "Sync"}
          </button>
        </div>
        {#if app.folders[acct.id]?.length}
          <ul class="folders">
            {#each app.folders[acct.id] as folder (folder.id)}
              <li>
                <button
                  class="folder"
                  class:current={isCurrent(acct.id, folder.name)}
                  onclick={() => selectFolder(acct.id, folder.name)}
                >
                  {folder.name}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </li>
    {:else}
      <li class="empty">No accounts yet</li>
    {/each}
  </ul>
</aside>

<style>
  .sidebar {
    width: 240px;
    border-right: 1px solid #ddd;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
  }
  .add {
    padding: 8px;
    cursor: pointer;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    padding: 6px 8px;
    border-radius: 6px;
  }
  li:not(.empty):hover {
    background: rgba(0, 0, 0, 0.05);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .sync {
    font-size: 0.75em;
    padding: 3px 8px;
    cursor: pointer;
  }
  .name {
    font-weight: 600;
  }
  .sub {
    font-size: 0.8em;
    opacity: 0.7;
  }
  .empty {
    opacity: 0.5;
    font-style: italic;
  }
  .folders {
    margin-top: 4px;
    gap: 2px;
  }
  .folders li {
    padding: 0;
  }
  .folder {
    width: 100%;
    text-align: left;
    padding: 3px 8px 3px 16px;
    border: none;
    background: none;
    cursor: pointer;
    font: inherit;
    font-size: 0.85em;
    opacity: 0.75;
    border-radius: 6px;
  }
  .folder:hover {
    background: rgba(0, 0, 0, 0.05);
  }
  .folder.current {
    font-weight: 600;
    opacity: 1;
  }
</style>
