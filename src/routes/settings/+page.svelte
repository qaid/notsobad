<script lang="ts">
  import { setThemePref, theme, type ThemePref } from "$lib/store.svelte";
</script>

<section class="settings">
  <h2>Settings</h2>

  <label>
    Appearance
    <select
      value={theme.pref}
      onchange={(e) => setThemePref(e.currentTarget.value as ThemePref)}
    >
      <option value="system">System</option>
      <option value="light">Light</option>
      <option value="dark">Dark</option>
    </select>
  </label>
</section>

<style>
  .settings {
    max-width: 480px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    font-size: 0.85em;
    gap: 3px;
  }
</style>
