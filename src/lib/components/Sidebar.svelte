<script lang="ts">
  import { app } from "../store.svelte";
</script>

<aside class="sidebar">
  <button class="add" onclick={() => (app.showWizard = true)}>+ Add account</button>
  <ul>
    {#each app.accounts as acct (acct.id)}
      <li>
        <div class="name">{acct.display_name}</div>
        <div class="sub">{acct.username}</div>
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
</style>
