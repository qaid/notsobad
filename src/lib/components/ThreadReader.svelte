<script lang="ts">
  import { app, closeThread } from "../store.svelte";
  import { messageBody } from "../ipc";
  import MessageBody from "./MessageBody.svelte";

  // Lazily-fetched bodies for meta_only messages, keyed by message id. Not
  // pushed back into app.currentThread to keep the store a plain server mirror.
  // A genuinely empty body is a valid fetched value, so presence is checked
  // with `in` (not truthiness) everywhere this map is read — a falsy check
  // would treat an empty-body message as "not fetched yet" forever and
  // re-issue a live IMAP fetch on every render.
  let fetchedBodies = $state<Record<number, { body: string; body_is_html: boolean }>>({});

  async function loadBody(messageId: number) {
    if (messageId in fetchedBodies) return;
    fetchedBodies[messageId] = await messageBody(messageId);
  }
</script>

<div class="thread">
  <button class="back" onclick={closeThread}>&larr; Back to inbox</button>
  {#each app.currentThread ?? [] as msg (msg.id)}
    <article class="message">
      <header>
        <div class="from">{msg.from_name || msg.from_addr || "(unknown sender)"}</div>
        <div class="subject">{msg.subject || "(no subject)"}</div>
        <div class="date">{msg.received_at || ""}</div>
      </header>
      {#if msg.body !== null}
        <MessageBody body={msg.body} isHtml={msg.body_is_html} />
      {:else if msg.id in fetchedBodies}
        <MessageBody body={fetchedBodies[msg.id].body} isHtml={fetchedBodies[msg.id].body_is_html} />
      {:else}
        <button class="load-body" onclick={() => loadBody(msg.id)}>Load message body</button>
      {/if}
    </article>
  {/each}
</div>

<style>
  .thread {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .back {
    align-self: flex-start;
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
    opacity: 0.7;
    padding: 4px 0;
  }
  .message {
    border: 1px solid #eee;
    border-radius: 8px;
    padding: 12px;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 8px;
  }
  .from {
    font-weight: 600;
  }
  .subject {
    flex: 1;
    opacity: 0.8;
  }
  .date {
    font-size: 0.8em;
    opacity: 0.6;
  }
  .load-body {
    cursor: pointer;
  }
</style>
