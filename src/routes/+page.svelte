<script lang="ts">
  import { api, type AppSettings, type AppUpdate, type Compat, type Job, type Phase } from "$lib/api";
  import Clone from "$lib/components/Clone.svelte";
  import Preflight from "$lib/components/Preflight.svelte";
  import Run from "$lib/components/Run.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import Sites from "$lib/components/Sites.svelte";
  import Wizard from "$lib/components/Wizard.svelte";
  import { listen } from "@tauri-apps/api/event";
  import AppSettingsDialog from "$lib/components/AppSettingsDialog.svelte";
  import { applyPreferences } from "$lib/preferences.svelte";
  import { appSettings, handleShortcut, modifier } from "$lib/shortcuts.svelte";

  type View = "preflight" | "wizard" | "sites" | "clone" | "defaults" | "run";

  let view = $state<View>("preflight");
  let previousView = $state<View>("wizard");
  let wizardKey = $state(0);

  /** Run stays mounted while you visit other views, because mounting it starts the job. */
  let activeRun = $state<{ id: number; job: Job; password: string | null } | null>(null);
  let runPhase = $state<Phase>("running");
  const runInProgress = $derived(activeRun !== null && runPhase === "running");

  const navItems = $derived<{ view: View; label: string }[]>([
    ...(activeRun ? [{ view: "run" as const, label: "Current setup" }] : []),
    { view: "wizard", label: "New site" },
    { view: "sites", label: "Sites" },
    { view: "clone", label: "Clone from Git" },
    { view: "defaults", label: "Project defaults" },
  ]);

  function go(next: View) {
    if (next === view) return;
    previousView = view;
    view = next;
  }
  $effect(applyPreferences);
  $effect(() => {
    const unlisten = listen("open-app-settings", () => (appSettings.open = true));
    return () => void unlisten.then((stop) => stop());
  });

  function startRun(job: Job, password: string | null) {
    runPhase = "running";
    activeRun = { id: (activeRun?.id ?? 0) + 1, job, password };
    go("run");
  }

  function leaveRun() {
    activeRun = null;
    wizardKey++;
    go("wizard");
  }

  const boot = Promise.all([api.compat(), api.loadSettings()]);
  let settings = $state<AppSettings | null>(null);
  let hasPassword = $state(false);
  let compat = $state<Compat | null>(null);
  boot.then(([c, s]) => {
    compat = c;
    settings = s.settings;
    hasPassword = s.hasPassword;
    if (s.settings.updateUrl.trim()) checkForUpdates();
  });

  let appUpdate = $state<AppUpdate | null>(null);

  async function reloadCompat() {
    compat = await api.compat();
    wizardKey++;
  }

  /** Quiet check on launch. Errors stay quiet too; Project defaults shows them. */
  async function checkForUpdates() {
    const [c, a] = await Promise.allSettled([api.checkCompatUpdate(), api.checkAppUpdate()]);
    if (c.status === "fulfilled" && c.value.updated) await reloadCompat();
    if (a.status === "fulfilled") appUpdate = a.value;
  }
</script>

<svelte:window onkeydown={handleShortcut} />

<a class="skip-link" href="#main-content">Skip to main content</a>

<header class="app-header">
  <div class="brand">
    <span class="brand-mark" aria-hidden="true">D</span>
    <div class="brand-copy">
      <h1 class="font-bold">Drupal Site Starter</h1>
      <small>Local Drupal workbench</small>
    </div>
  </div>
  <div class="header-actions">
    <nav class="app-nav" aria-label="Primary">
      {#if appUpdate}
        <button class="nav-button nav-button--update" onclick={() => go("defaults")}>Update to {appUpdate.version}</button>
      {/if}
      {#each navItems as item (item.view)}
        <button class="nav-button" aria-current={view === item.view ? "page" : undefined} onclick={() => go(item.view)}>{item.label}</button>
      {/each}
    </nav>
    <button
      class="icon-button"
      aria-label="App settings"
      aria-keyshortcuts="{modifier.aria}+,"
      title="App settings ({modifier.label},)"
      onclick={() => (appSettings.open = true)}
    >
      <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
        <circle cx="12" cy="12" r="3" />
      </svg>
    </button>
  </div>
</header>

<main id="main-content" class="app-main" tabindex="-1">
  {#await boot}
    <p class="text-sm help">Loading…</p>
  {:then}
    {#if compat && settings}
      {#if view === "preflight"}
        <Preflight projectsDir={settings.projectsDir} onContinue={() => go("wizard")} />
      {:else if view === "defaults"}
        <Settings
          {compat}
          {settings}
          {hasPassword}
          onCancel={() => go(previousView)}
          onCompatUpdated={reloadCompat}
          onSaved={(s, p) => {
            settings = s;
            hasPassword = p;
            wizardKey++;
            go(previousView);
          }}
        />
      {:else if view === "sites"}
        <Sites onNew={() => go("wizard")} />
      {:else if view === "clone"}
        <Clone {compat} {settings} {hasPassword} {runInProgress} onStart={startRun} onCancel={() => go("wizard")} />
      {:else if view === "wizard"}
        {#key wizardKey}
          <Wizard
            {compat}
            {settings}
            {hasPassword}
            {runInProgress}
            onCreate={(input, password) => startRun({ kind: "create", input }, password)}
          />
        {/key}
      {/if}
      {#if activeRun}
        {#key activeRun.id}
          <div hidden={view !== "run"}>
            <Run job={activeRun.job} password={activeRun.password} bind:phase={runPhase} onBack={leaveRun} />
          </div>
        {/key}
      {/if}
    {/if}
  {:catch e}
    <p class="error">{String(e)}</p>
  {/await}
</main>

<AppSettingsDialog />
