<script lang="ts">
  import { api, type AppSettings, type Compat, type ProjectInput } from "$lib/api";
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
    | { kind: "run"; input: ProjectInput; password: string | null };

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
  });
</script>

<header class="flex items-center justify-between border-b border-slate-200 bg-white px-6 py-3 dark:border-slate-800 dark:bg-slate-900">
  <div class="flex items-center gap-2">
    <span class="grid size-7 place-items-center rounded-md bg-drop text-sm font-bold text-white">D</span>
    <h1 class="text-sm font-semibold">Drupal DDEV Site Creator</h1>
  </div>
  {#if screen.kind !== "run" && screen.kind !== "settings"}
    <nav class="flex gap-4">
      {#if screen.kind === "wizard"}
        <button class="text-sm text-slate-500 hover:text-drop" onclick={() => (screen = { kind: "sites" })}>Sites</button>
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
          onSaved={(s, p) => {
            settings = s;
            hasPassword = p;
            wizardKey++;
            screen = back;
          }}
        />
      {:else if screen.kind === "sites"}
        <Sites onNew={() => (screen = { kind: "wizard" })} />
      {:else if screen.kind === "wizard"}
        {#key wizardKey}
          <Wizard {compat} {settings} {hasPassword} onCreate={(input, password) => (screen = { kind: "run", input, password })} />
        {/key}
      {:else}
        <Run
          input={screen.input}
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
