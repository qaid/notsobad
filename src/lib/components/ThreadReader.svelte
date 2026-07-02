<script lang="ts">
  import { app, closeThread, translateAndCache, toggleShowOriginal } from "../store.svelte";
  import { messageBody } from "../ipc";
  import MessageBody from "./MessageBody.svelte";

  // Lazily-fetched bodies for meta_only messages, keyed by message id. Not
  // pushed back into app.currentThread to keep the store a plain server mirror.
  // A genuinely empty body is a valid fetched value, so presence is checked
  // with `in` (not truthiness) everywhere this map is read — a falsy check
  // would treat an empty-body message as "not fetched yet" forever and
  // re-issue a live IMAP fetch on every render.
  let fetchedBodies = $state<Record<number, { body: string; body_is_html: boolean }>>({});
  let bodyLoadInFlight = new Set<number>();
  let translateErrors = $state<Record<number, string>>({});

  async function loadBody(messageId: number) {
    if (messageId in fetchedBodies || bodyLoadInFlight.has(messageId)) return;
    bodyLoadInFlight.add(messageId);
    try {
      fetchedBodies[messageId] = await messageBody(messageId);
    } finally {
      bodyLoadInFlight.delete(messageId);
    }
    // Body just became available — translate now rather than waiting for a
    // second click, matching the "opening a message translates it" AC.
    runTranslate(messageId);
  }

  function runTranslate(messageId: number) {
    translateAndCache(messageId)
      .then(() => {
        delete translateErrors[messageId];
      })
      .catch((e) => {
        console.error("translate failed", e);
        translateErrors[messageId] = String(e);
      });
  }

  // On-open timing (#5): every synced message is stored meta_only (headers
  // only, no body — see connection/sync.rs) to avoid an eager IMAP body fetch
  // per message, so msg.body is ALWAYS null straight from thread_messages in
  // practice; loadBody is what actually fetches it. This effect drives that
  // fetch automatically on open instead of waiting for the user to notice and
  // click "Load message body" — that manual-click gate was the reason #5's
  // first device test found translate never ran on any real mail. The
  // msg.body !== null branch is kept for a hypothetical full-mirror row (e.g.
  // a future eager-sync mode) so this doesn't silently skip a body that's
  // already there.
  $effect(() => {
    for (const msg of app.currentThread ?? []) {
      if (msg.body !== null) {
        runTranslate(msg.id);
      } else if (!(msg.id in fetchedBodies)) {
        loadBody(msg.id);
      }
    }
  });
</script>

<div class="thread">
  <button class="back" onclick={closeThread}>&larr; Back to inbox</button>
  {#each app.currentThread ?? [] as msg (msg.id)}
    {@const original = msg.body !== null ? msg.body : fetchedBodies[msg.id]?.body}
    {@const isHtml = msg.body !== null ? msg.body_is_html : fetchedBodies[msg.id]?.body_is_html}
    {@const translation = app.translations[msg.id]}
    {@const hasRealTranslation = translation?.translated != null}
    <article class="message">
      <header>
        <div class="from">{msg.from_name || msg.from_addr || "(unknown sender)"}</div>
        <div class="subject">{msg.subject || "(no subject)"}</div>
        <div class="date">{msg.received_at || ""}</div>
      </header>
      {#if translation?.pull_hint}
        <p class="pull-hint">{translation.pull_hint}</p>
      {:else if translateErrors[msg.id]}
        <p class="pull-hint">Translation failed: {translateErrors[msg.id]}</p>
      {/if}
      {#if original !== undefined}
        {#if hasRealTranslation && !app.showOriginal[msg.id]}
          <!-- Inline-replaced by default: translated text is always plain
               text (the model translates prose, not markup), regardless of
               whether the original was HTML. `translated` is only non-null
               here (hasRealTranslation), never a same-as-original stand-in —
               see TranslationResult's doc comment for why that distinction
               matters for HTML mail. -->
          <MessageBody body={translation.translated ?? ""} isHtml={false} />
        {:else}
          <!-- No real translation (already-English, or model-not-pulled) —
               render the original natively with its own body_is_html, never
               through the plain-text translated-view branch. -->
          <MessageBody body={original} isHtml={isHtml ?? false} />
        {/if}
        {#if hasRealTranslation}
          <button class="toggle-original" onclick={() => toggleShowOriginal(msg.id)}>
            {app.showOriginal[msg.id] ? "Show translation" : "Show original"}
          </button>
        {/if}
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
  .toggle-original {
    margin-top: 8px;
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
    font-size: 0.85em;
    opacity: 0.7;
    padding: 4px 0;
    text-decoration: underline;
  }
  .pull-hint {
    background: #fff8e1;
    border: 1px solid #f0d878;
    border-radius: 6px;
    font-size: 0.9em;
    margin: 0 0 8px 0;
    padding: 8px 10px;
  }
</style>
