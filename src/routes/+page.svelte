<script lang="ts">
  import AccountWizard from "$lib/components/AccountWizard.svelte";
  import InboxList from "$lib/components/InboxList.svelte";
  import ThreadReader from "$lib/components/ThreadReader.svelte";
  import { app } from "$lib/store.svelte";
</script>

{#if app.showWizard}
  <AccountWizard />
{:else if app.currentThread}
  <ThreadReader />
{:else if app.accounts.length === 0}
  <div class="placeholder">
    <h1>notsobad</h1>
    <p>Add an account from the sidebar to get started.</p>
  </div>
{:else}
  <InboxList />
{/if}

<style>
  .placeholder {
    opacity: 0.6;
  }
</style>
