<script lang="ts">
  import { untrack } from "svelte";
  import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import PasswordInput from "./PasswordInput.svelte";
  import { api, type AppSettings, type AppUpdate, type Compat, type Preset } from "$lib/api";

  let {
    compat,
    settings,
    hasPassword,
    onSaved,
    onCancel,
    onCompatUpdated,
  }: {
    compat: Compat;
    settings: AppSettings;
    hasPassword: boolean;
    onSaved: (s: AppSettings, hasPassword: boolean) => void;
    onCancel: () => void;
    onCompatUpdated: () => void;
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

  const PRESET_FILES = [{ name: "Presets", extensions: ["yaml", "yml", "json"] }];
  let presets = $state<Preset[]>([]);
  let presetNotice = $state("");
  api.listPresets().then((p) => (presets = p));

  async function presetAction(action: () => Promise<string | void>) {
    presetNotice = "";
    try {
      presetNotice = (await action()) ?? "";
    } catch (e) {
      presetNotice = String(e);
    }
  }

  const importPresets = () =>
    presetAction(async () => {
      const path = await open({ filters: PRESET_FILES });
      if (typeof path !== "string") return;
      presets = await api.importPresets(path);
      return `Imported presets from ${path}.`;
    });

  const exportPresets = () =>
    presetAction(async () => {
      const path = await saveDialog({ defaultPath: "presets.yaml", filters: PRESET_FILES });
      if (!path) return;
      await api.exportPresets(path);
      return `Exported ${presets.length} presets to ${path}.`;
    });

  let checking = $state(false);
  let updateNotes = $state<string[]>([]);
  let appUpdate = $state<AppUpdate | null>(null);
  let installing = $state(false);

  /** Saves first, so the checks use the URL as typed. */
  async function checkUpdates() {
    checking = true;
    updateNotes = [];
    appUpdate = null;
    try {
      await api.saveSettings(draft, null);
      const [c, a] = await Promise.allSettled([api.checkCompatUpdate(), api.checkAppUpdate()]);
      if (c.status === "fulfilled") {
        updateNotes.push(c.value.updated ? `Drupal and service data updated to revision ${c.value.revision}.` : `Drupal and service data are up to date (revision ${c.value.revision}).`);
        if (c.value.updated) onCompatUpdated();
      } else updateNotes.push(String(c.reason));
      if (a.status === "fulfilled") {
        appUpdate = a.value;
        if (!a.value) updateNotes.push("The app is up to date.");
      } else updateNotes.push(String(a.reason));
    } catch (e) {
      updateNotes.push(String(e));
    }
    checking = false;
  }

  async function installUpdate() {
    installing = true;
    try {
      await api.installAppUpdate();
    } catch (e) {
      updateNotes.push(String(e));
      installing = false;
    }
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
    <label for="update-url">Update URL</label>
    <div class="flex gap-2">
      <input id="update-url" class="input flex-1 font-mono" bind:value={draft.updateUrl} placeholder="https://example.com/drupal-ddev-creator" />
      <button class="btn-ghost" disabled={!draft.updateUrl.trim() || checking} onclick={checkUpdates}>
        {checking ? "Checking…" : "Check now"}
      </button>
    </div>
    <small class="text-slate-500">
      The app reads compat.json (Drupal, PHP and service data, revision {compat.revision}) and latest.json (app updates) from here. Leave it empty to never check.
    </small>
    {#each updateNotes as note}<small class="text-slate-500">{note}</small>{/each}
    {#if appUpdate}
      <div class="flex items-center justify-between rounded-md border border-drop/40 bg-drop/5 px-3 py-2 text-sm">
        <span>Version {appUpdate.version} is available (you have {appUpdate.currentVersion}).</span>
        <button class="btn-primary" disabled={installing} onclick={installUpdate}>{installing ? "Installing…" : "Install and restart"}</button>
      </div>
    {/if}
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

  <div class="field">
    <div class="flex items-center justify-between">
      <span>Presets</span>
      <div class="flex gap-2">
        <button class="btn-ghost" onclick={importPresets}>Import…</button>
        <button class="btn-ghost" disabled={!presets.length} onclick={exportPresets}>Export…</button>
      </div>
    </div>
    {#if presets.length}
      <ul class="divide-y divide-slate-200 rounded-md border border-slate-200 text-sm dark:divide-slate-800 dark:border-slate-800">
        {#each presets as p (p.name)}
          {@const label = compat.drupal.find((r) => r.id === p.drupal)?.label ?? p.drupal}
          <li class="flex items-center justify-between px-3 py-2">
            <span><span class="font-medium">{p.name}</span> <span class="text-xs text-slate-500">{label}, PHP {p.php}, {p.database.type} {p.database.version}</span></span>
            <button class="text-xs text-red-600 hover:underline" onclick={() => presetAction(async () => void (presets = await api.deletePreset(p.name)))}>Delete</button>
          </li>
        {/each}
      </ul>
    {:else}
      <small class="text-slate-500">Save a preset from the wizard's Review step, or import one.</small>
    {/if}
    {#if presetNotice}<small class="text-slate-500">{presetNotice}</small>{/if}
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  <div class="flex justify-end gap-2">
    <button class="btn-ghost" onclick={onCancel}>Cancel</button>
    <button class="btn-primary" onclick={save}>Save</button>
  </div>
</section>
