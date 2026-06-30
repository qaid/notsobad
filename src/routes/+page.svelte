<script lang="ts">
  import AccountWizard from "$lib/components/AccountWizard.svelte";
  import { app } from "$lib/store.svelte";
</script>

{#if app.showWizard}
  <AccountWizard />
{:else}
  <div class="placeholder">
    <h1>notsobad</h1>
    <p>Add an account from the sidebar to get started.</p>
  </div>
{/if}

<style>
  .placeholder {
    opacity: 0.6;
  }
</style>
