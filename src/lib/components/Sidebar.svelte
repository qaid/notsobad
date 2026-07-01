<script lang="ts">
  import { app, selectFolder, selectUnifiedInbox, syncAndRefresh, toggleFolderSelected } from "../store.svelte";

  let syncing = $state<number | null>(null);

  async function sync(accountId: number) {
    syncing = accountId;
    try {
      await syncAndRefresh(accountId);
    } finally {
      syncing = null;
    }
  }

  // Account-scoped: currentFolder is always {accountId, name} for a specific
  // folder selection (see store.svelte's doc comment), so this can tell one
  // account's INBOX from another's — previously `name === "INBOX" ->
  // app.currentFolder === null` ignored accountId entirely and highlighted
  // every account's INBOX row at once. Since currentFolder is only ever null
  // when the unified view is active (never as a stand-in for "this
  // account's INBOX"), no account's INBOX row can highlight while the
  // unified view is selected, and vice versa.
  function isCurrent(accountId: number, name: string) {
    return app.currentFolder?.accountId === accountId && app.currentFolder?.name === name;
  }

  function isUnifiedCurrent() {
    return app.currentFolder === null;
  }

  function toggleSelected(accountId: number, name: string, event: Event) {
    const checked = (event.currentTarget as HTMLInputElement).checked;
    void toggleFolderSelected(accountId, name, checked);
  }
</script>

<aside class="sidebar">
  <button class="add" onclick={() => (app.showWizard = true)}>+ Add account</button>
  <button class="unified" class:current={isUnifiedCurrent()} onclick={() => selectUnifiedInbox()}>
    All Inboxes
  </button>
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
              <li class="folder-row">
                <input
                  type="checkbox"
                  class="folder-toggle"
                  checked={folder.selected}
                  title={folder.selected ? "Synced — uncheck to stop syncing" : "Not synced — check to sync"}
                  onchange={(e) => toggleSelected(acct.id, folder.name, e)}
                />
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
  .unified {
    text-align: left;
    padding: 8px;
    border: none;
    background: none;
    cursor: pointer;
    font: inherit;
    border-radius: 6px;
  }
  .unified:hover {
    background: rgba(0, 0, 0, 0.05);
  }
  .unified.current {
    font-weight: 700;
    background: rgba(0, 0, 0, 0.12);
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
  .folder-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-left: 8px;
  }
  .folder-toggle {
    flex-shrink: 0;
    cursor: pointer;
  }
  .folder {
    flex: 1;
    text-align: left;
    padding: 3px 8px 3px 8px;
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
  /* "Currently showing" state: a filled row background + bold, deliberately
     stronger than hover and visually distinct from the sync checkbox — the
     checkbox means "sync this folder", this fill means "this folder's mail is
     in the content pane right now". */
  .folder.current {
    font-weight: 700;
    opacity: 1;
    background: rgba(0, 0, 0, 0.12);
  }
  .folder.current:hover {
    background: rgba(0, 0, 0, 0.12);
  }
</style>
