<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import PasswordInput from "./PasswordInput.svelte";
  import { api, type AppSettings, type Compat } from "$lib/api";

  let {
    compat,
    settings,
    hasPassword,
    onSaved,
    onCancel,
  }: {
    compat: Compat;
    settings: AppSettings;
    hasPassword: boolean;
    onSaved: (s: AppSettings, hasPassword: boolean) => void;
    onCancel: () => void;
  } = $props();

  let draft = $state(untrack(() => $state.snapshot(settings)));
  let password = $state("");
  let error = $state("");

  async function pickFolder() {
    const dir = await open({ directory: true, defaultPath: draft.projectsDir });
    if (typeof dir === "string") draft.projectsDir = dir;
  }

  function toggle(id: string) {
    draft.services = draft.services.includes(id)
      ? draft.services.filter((s) => s !== id)
      : [...draft.services, id];
  }

  async function save() {
    try {
      await api.saveSettings(draft, password || null);
      onSaved(draft, hasPassword || password.length > 0);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="card mx-auto max-w-2xl space-y-4">
  <h2 class="text-lg font-semibold">Settings</h2>

  <div class="grid grid-cols-2 gap-4">
    <label class="field"><span>Admin username</span><input class="input" bind:value={draft.adminName} /></label>
    <label class="field"><span>Admin email</span><input class="input" type="email" bind:value={draft.adminEmail} /></label>
  </div>
  <div class="field">
    <label for="settings-admin-password">Admin password</label>
    <PasswordInput
      id="settings-admin-password"
      bind:value={password}
      placeholder={hasPassword ? "Saved in the keychain. Type to replace it." : "Stored in the OS keychain"}
    />
  </div>

  <div class="field">
    <span>Projects folder</span>
    <div class="flex gap-2">
      <input class="input flex-1" bind:value={draft.projectsDir} />
      <button class="btn-ghost" onclick={pickFolder}>Choose…</button>
    </div>
  </div>

  <div class="grid grid-cols-2 gap-4">
    <label class="field"><span>Config sync path</span><input class="input" bind:value={draft.configSyncDir} /></label>
    <label class="field"><span>IDE command</span><input class="input" bind:value={draft.ide} /></label>
  </div>

  <div class="field">
    <span>Default services</span>
    <div class="flex flex-wrap gap-2">
      {#each compat.services as s (s.id)}
        <button
          class="rounded-full border px-3 py-1 text-xs {draft.services.includes(s.id)
            ? 'border-drop bg-drop text-white'
            : 'border-slate-300 dark:border-slate-700'}"
          onclick={() => toggle(s.id)}>{s.label}</button
        >
      {/each}
    </div>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  <div class="flex justify-end gap-2">
    <button class="btn-ghost" onclick={onCancel}>Cancel</button>
    <button class="btn-primary" onclick={save}>Save</button>
  </div>
</section>
