<script lang="ts">
  import { app, openThread, refreshInbox } from "../store.svelte";
  import { onMount } from "svelte";

  onMount(refreshInbox);
</script>

<div class="inbox">
  {#each app.inbox as msg (msg.id)}
    <button class="row" class:unread={!msg.seen} onclick={() => openThread(msg.thread_id)}>
      <div class="from">{msg.from_name || msg.from_addr || "(unknown sender)"}</div>
      <div class="subject">{msg.subject || "(no subject)"}</div>
      <div class="snippet">{msg.snippet || ""}</div>
      <div class="date">{msg.received_at?.slice(0, 10) || ""}</div>
    </button>
  {:else}
    <p class="empty">No mail yet. Sync an account from the sidebar.</p>
  {/each}
</div>

<style>
  .inbox {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: grid;
    grid-template-columns: 160px 1fr auto;
    grid-template-rows: auto auto;
    gap: 2px 12px;
    text-align: left;
    padding: 10px 8px;
    border: none;
    border-bottom: 1px solid #eee;
    background: none;
    cursor: pointer;
    font: inherit;
  }
  .row:hover {
    background: rgba(0, 0, 0, 0.04);
  }
  .row.unread .from,
  .row.unread .subject {
    font-weight: 700;
  }
  .from {
    grid-column: 1;
    grid-row: 1;
  }
  .subject {
    grid-column: 2;
    grid-row: 1;
  }
  .date {
    grid-column: 3;
    grid-row: 1;
    font-size: 0.8em;
    opacity: 0.6;
  }
  .snippet {
    grid-column: 1 / span 3;
    grid-row: 2;
    font-size: 0.85em;
    opacity: 0.6;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    opacity: 0.6;
  }
</style>
