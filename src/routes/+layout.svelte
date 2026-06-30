<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { refreshAccounts } from "$lib/store.svelte";

  let { children } = $props();
  onMount(refreshAccounts);
</script>

<div class="app">
  <Sidebar />
  <main>{@render children()}</main>
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
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
