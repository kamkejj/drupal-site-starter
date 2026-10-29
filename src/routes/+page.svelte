<script lang="ts">
  import { api, type AppSettings, type AppUpdate, type Compat, type Job } from "$lib/api";
  import Clone from "$lib/components/Clone.svelte";
  import Preflight from "$lib/components/Preflight.svelte";
  import Run from "$lib/components/Run.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import Sites from "$lib/components/Sites.svelte";
  import Wizard from "$lib/components/Wizard.svelte";

  type Screen =
    | { kind: "preflight" }
    | { kind: "settings"; back: Screen }
    | { kind: "wizard" }
    | { kind: "sites" }
    | { kind: "clone" }
    | { kind: "run"; job: Job; password: string | null };

  let screen = $state<Screen>({ kind: "preflight" });
  let wizardKey = $state(0);

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

  /** Quiet check on launch. Errors stay quiet too; Settings shows them. */
  async function checkForUpdates() {
    const [c, a] = await Promise.allSettled([api.checkCompatUpdate(), api.checkAppUpdate()]);
    if (c.status === "fulfilled" && c.value.updated) await reloadCompat();
    if (a.status === "fulfilled") appUpdate = a.value;
  }
</script>

<header class="flex items-center justify-between border-b border-slate-200 bg-white px-6 py-3 dark:border-slate-800 dark:bg-slate-900">
  <div class="flex items-center gap-2">
    <span class="grid size-7 place-items-center rounded-md bg-drop text-sm font-bold text-white">D</span>
    <h1 class="text-sm font-semibold">Drupal Site Starter</h1>
  </div>
  {#if screen.kind !== "run" && screen.kind !== "settings"}
    <nav class="flex gap-4">
      {#if appUpdate}
        <button class="text-sm font-medium text-drop" onclick={() => (screen = { kind: "settings", back: screen })}>
          Update to {appUpdate.version}
        </button>
      {/if}
      {#if screen.kind !== "wizard" && screen.kind !== "preflight"}
        <button class="text-sm text-slate-500 hover:text-drop" onclick={() => (screen = { kind: "wizard" })}>New site</button>
      {/if}
      {#if screen.kind !== "sites"}
        <button class="text-sm text-slate-500 hover:text-drop" onclick={() => (screen = { kind: "sites" })}>Sites</button>
      {/if}
      {#if screen.kind !== "clone"}
        <button class="text-sm text-slate-500 hover:text-drop" onclick={() => (screen = { kind: "clone" })}>Clone from Git</button>
      {/if}
      <button class="text-sm text-slate-500 hover:text-drop" onclick={() => (screen = { kind: "settings", back: screen })}>
        Settings
      </button>
    </nav>
  {/if}
</header>

<main class="p-6">
  {#await boot}
    <p class="text-sm text-slate-500">Loading…</p>
  {:then}
    {#if compat && settings}
      {#if screen.kind === "preflight"}
        <Preflight projectsDir={settings.projectsDir} onContinue={() => (screen = { kind: "wizard" })} />
      {:else if screen.kind === "settings"}
        {@const back = screen.back}
        <Settings
          {compat}
          {settings}
          {hasPassword}
          onCancel={() => (screen = back)}
          onCompatUpdated={reloadCompat}
          onSaved={(s, p) => {
            settings = s;
            hasPassword = p;
            wizardKey++;
            screen = back;
          }}
        />
      {:else if screen.kind === "sites"}
        <Sites onNew={() => (screen = { kind: "wizard" })} />
      {:else if screen.kind === "clone"}
        <Clone
          {compat}
          {settings}
          {hasPassword}
          onStart={(job, password) => (screen = { kind: "run", job, password })}
          onCancel={() => (screen = { kind: "wizard" })}
        />
      {:else if screen.kind === "wizard"}
        {#key wizardKey}
          <Wizard
            {compat}
            {settings}
            {hasPassword}
            onCreate={(input, password) => (screen = { kind: "run", job: { kind: "create", input }, password })}
          />
        {/key}
      {:else}
        <Run
          job={screen.job}
          password={screen.password}
          onBack={() => {
            wizardKey++;
            screen = { kind: "wizard" };
          }}
        />
      {/if}
    {/if}
  {:catch e}
    <p class="error">{String(e)}</p>
  {/await}
</main>
