<script lang="ts">
  // Renders a mail body without letting it phone home or run script (CLAUDE.md:
  // no mail content leaves the machine; a tracking pixel must not beacon out).
  // `sandbox` with no `allow-scripts` kills JS/XSS; the CSP blocks remote
  // img/style/font loads so only inline styles and data: images render.
  let { body }: { body: string } = $props();

  const isHtml = $derived(/<\s*(html|body|div|table|p|br)[\s>]/i.test(body));

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
