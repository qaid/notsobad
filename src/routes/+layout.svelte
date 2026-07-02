<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { applyTheme, refreshAccounts, theme } from "$lib/store.svelte";

  let { children } = $props();

  onMount(() => {
    refreshAccounts();
    applyTheme();
    // Live-update when the OS setting changes, but only while following it —
    // a manual override must not be clobbered by an OS change.
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => theme.pref === "system" && applyTheme();
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  });
</script>

<div class="app">
  <Sidebar />
  <main>{@render children()}</main>
</div>

<style>
  :global(:root) {
    color-scheme: light;
    --bg: #fff;
    --fg: #111;
    --border: #ddd;
    --hover: rgba(0, 0, 0, 0.05);
    --active: rgba(0, 0, 0, 0.12);
    --accent-ok: green;
    --accent-err: #c00;
  }
  :global([data-theme="dark"]) {
    color-scheme: dark;
    --bg: #1e1e1e;
    --fg: #eee;
    --border: #444;
    --hover: rgba(255, 255, 255, 0.08);
    --active: rgba(255, 255, 255, 0.15);
    --accent-ok: #4caf50;
    --accent-err: #e57373;
  }
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    background: var(--bg);
    color: var(--fg);
  }
  .app {
    display: flex;
    height: 100vh;
  }
  main {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
  }
</style>
