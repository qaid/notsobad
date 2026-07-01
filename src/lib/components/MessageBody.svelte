<script lang="ts">
  // Renders a mail body without letting it phone home or run script (CLAUDE.md:
  // no mail content leaves the machine; a tracking pixel must not beacon out).
  // `sandbox` with no `allow-scripts` kills JS/XSS; the CSP blocks remote
  // img/style/font loads so only inline styles and data: images render.
  //
  // isHtml comes from the backend's own parse (mail-parser's body_html vs
  // body_text), not a frontend tag-regex guess — a guess misclassifies real
  // HTML mail that only uses tags like <a>/<img>/<span> and renders it as
  // raw escaped markup instead of formatted content.
  let { body, isHtml }: { body: string; isHtml: boolean } = $props();

  const srcdoc = $derived(
    isHtml
      ? `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'"><base target="_blank"></head><body>${body}</body></html>`
      : "",
  );
</script>

{#if isHtml}
  <iframe title="message body" sandbox="" srcdoc={srcdoc}></iframe>
{:else}
  <pre>{body}</pre>
{/if}

<style>
  iframe {
    width: 100%;
    min-height: 300px;
    border: none;
  }
  pre {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    margin: 0;
  }
</style>
