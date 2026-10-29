<script lang="ts">
  import { tick, untrack } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import PasswordInput from "./PasswordInput.svelte";
  import {
    api,
    availableFor,
    fieldErrors,
    type AppSettings,
    type Ci,
    type Compat,
    type DbType,
    type FieldError,
    type Plan,
    type Preset,
    type ProjectInput,
  } from "$lib/api";

  let {
    compat,
    settings,
    hasPassword,
    onCreate,
    runInProgress,
  }: {
    compat: Compat;
    settings: AppSettings;
    hasPassword: boolean;
    runInProgress: boolean;
    onCreate: (input: ProjectInput, password: string | null) => void;
  } = $props();

  const STEPS = ["Project", "Drupal and PHP", "Environment", "Services", "Review"] as const;
  const FIELD_STEP: Record<keyof ProjectInput, number> = {
    name: 0, parentDir: 0, siteName: 0, adminName: 0, adminEmail: 0,
    drupal: 1, php: 1,
    webserver: 2, httpsPort: 2, database: 2, configSyncDir: 2,
    services: 3, bundles: 3, recipes: 3, extraModules: 3, ci: 3, exportConfigOnInstall: 3, initGit: 3,
  };
  const CI_OPTIONS: [Ci, string][] = [["none", "None"], ["github", "GitHub Actions"], ["gitlab", "GitLab CI"]];
  const DB_LABEL: Record<DbType, string> = { mariadb: "MariaDB", mysql: "MySQL", postgres: "PostgreSQL" };

  const seed = untrack(() => ({
    first: compat.drupal.find((r) => r.status === "supported") ?? compat.drupal[0],
    defaults: $state.snapshot(settings),
  }));
  const first = seed.first;
  let stepIndex = $state(0);
  let displayName = $state("");
  let password = $state("");
  let taken = $state<string[]>([]);
  let plan = $state<Plan | null>(null);
  let errors = $state<FieldError[]>([]);
  let showScript = $state(false);
  let notice = $state("");
  let extraText = $state("");

  let input = $state<ProjectInput>({
    name: "",
    parentDir: seed.defaults.projectsDir,
    drupal: first.id,
    php: first.defaultPhp,
    webserver: "nginx-fpm",
    database: { ...first.defaultDatabase },
    services: [...seed.defaults.services],
    bundles: [],
    recipes: [],
    extraModules: [],
    ci: "none",
    adminName: seed.defaults.adminName,
    adminEmail: seed.defaults.adminEmail,
    siteName: "",
    configSyncDir: seed.defaults.configSyncDir,
    exportConfigOnInstall: true,
    initGit: true,
    httpsPort: null,
  });

  let presets = $state<Preset[]>([]);
  let presetName = $state("");
  let presetNote = $state("");
  let newPresetName = $state("");
  const activePreset = $derived(presets.find((p) => p.name === presetName));

  api.listPresets().then((p) => (presets = p));

  function applyPreset(name: string) {
    presetName = name;
    presetNote = "";
    const p = presets.find((x) => x.name === name);
    if (!p) return;
    const r = compat.drupal.find((d) => d.id === p.drupal);
    if (!r) {
      presetNote = `${p.name} uses Drupal "${p.drupal}", which this version of the app doesn't offer.`;
      return;
    }
    const phpOk = r.php.includes(p.php);
    const dbOk = r.databases[p.database.type]?.includes(p.database.version) ?? false;
    input.drupal = r.id;
    input.php = phpOk ? p.php : r.defaultPhp;
    input.database = dbOk ? { ...p.database } : { ...r.defaultDatabase };
    input.webserver = p.webserver;
    const dropped: string[] = [];
    /** Keeps the ids `ok` accepts and names the rest in `dropped`. */
    const keep = (ids: string[], all: { id: string; label: string }[], ok: (id: string) => boolean) =>
      ids.filter((id) => ok(id) || void dropped.push(all.find((x) => x.id === id)?.label ?? id));
    input.services = keep(p.services, compat.services, (id) => compat.services.some((s) => s.id === id));
    input.bundles = keep(p.bundles ?? [], compat.bundles, (id) => compat.bundles.some((b) => b.id === id && availableFor(b, r.id)));
    input.recipes = keep(p.recipes ?? [], compat.recipes, (id) => compat.recipes.some((x) => x.id === id && availableFor(x, r.id)));
    setExtraModules((p.extraModules ?? []).join(", "));
    input.ci = p.ci ?? "none";
    input.configSyncDir = p.configSyncDir;
    input.exportConfigOnInstall = p.exportConfigOnInstall;
    input.initGit = p.initGit;
    const notes: string[] = [];
    if (!phpOk || !dbOk) notes.push(`${r.label} no longer supports some of this preset's versions, so the defaults are used instead.`);
    if (dropped.length) notes.push(`Left out because they aren't available for ${r.label}: ${dropped.join(", ")}.`);
    presetNote = notes.join(" ");
  }

  async function savePreset() {
    const { drupal, php, webserver, database, services, bundles, recipes, extraModules, ci, configSyncDir, exportConfigOnInstall, initGit } =
      $state.snapshot(input);
    const name = newPresetName.trim();
    try {
      presets = await api.savePreset({
        name, drupal, php, webserver, database, services, bundles, recipes, extraModules, ci, configSyncDir, exportConfigOnInstall, initGit,
      });
      presetName = name;
      newPresetName = "";
      notice = `Saved the preset ${name}.`;
    } catch (e) {
      notice = String(e);
    }
  }

  const release = $derived(compat.drupal.find((r) => r.id === input.drupal) ?? first);
  const dbTypes = $derived(Object.keys(release.databases) as DbType[]);
  const bundles = $derived(compat.bundles.filter((b) => availableFor(b, release.id)));
  const recipes = $derived(compat.recipes.filter((r) => availableFor(r, release.id)));
  const nameTaken = $derived(taken.includes(input.name));

  api.existingProjects().then((names) => (taken = names));

  let portUser = $state<string | null>(null);
  $effect(() => {
    const port = input.httpsPort;
    portUser = null;
    if (port == null || !Number.isInteger(port) || port < 1024 || port > 65535) return;
    api.portUser(port).then((user) => {
      if (input.httpsPort === port) portUser = user;
    });
  });

  $effect(() => {
    const display = displayName;
    api.machineName(display).then((n) => (input.name = n));
  });

  function chooseRelease(id: string) {
    const r = compat.drupal.find((d) => d.id === id)!;
    input.drupal = id;
    if (!r.php.includes(input.php)) input.php = r.defaultPhp;
    if (!r.databases[input.database.type]?.includes(input.database.version)) {
      input.database = { ...r.defaultDatabase };
    }
    input.bundles = input.bundles.filter((b) => compat.bundles.some((x) => x.id === b && availableFor(x, id)));
    input.recipes = input.recipes.filter((b) => compat.recipes.some((x) => x.id === b && availableFor(x, id)));
  }

  function toggle(list: "bundles" | "recipes", id: string) {
    input[list] = input[list].includes(id) ? input[list].filter((x) => x !== id) : [...input[list], id];
  }

  function setExtraModules(text: string) {
    extraText = text;
    input.extraModules = text.split(/[\s,]+/).filter(Boolean);
  }

  function chooseDbType(type: DbType) {
    const versions = release.databases[type] ?? [];
    input.database = { type, version: versions.at(-1) ?? "" };
  }

  function toggleService(id: string) {
    input.services = input.services.includes(id)
      ? input.services.filter((s) => s !== id)
      : [...input.services, id];
  }

  async function pickParent() {
    const dir = await open({ directory: true, defaultPath: input.parentDir });
    if (typeof dir === "string") input.parentDir = dir;
  }

  function errorFor(field: FieldError["field"]) {
    return errors.find((e) => e.field === field)?.message;
  }

  async function goTo(i: number) {
    stepIndex = i;
    if (STEPS[i] !== "Review") {
      await tick();
      document.getElementById("wizard-heading")?.focus();
      return;
    }
    if (!input.siteName.trim()) input.siteName = displayName.trim();
    try {
      plan = await api.plan({ kind: "create", input });
      errors = [];
    } catch (e) {
      plan = null;
      errors = fieldErrors(e);
      if (errors.length) {
        stepIndex = Math.min(...errors.map((x) => FIELD_STEP[x.field as keyof ProjectInput] ?? 0));
        await tick();
        document.querySelector<HTMLElement>("#wizard [aria-invalid='true']")?.focus();
      }
    }
  }

  async function exportScript() {
    const path = await save({ defaultPath: `${input.parentDir}/create-${input.name}.sh` });
    if (!path) return;
    await api.exportScript(input, path);
    notice = `Saved ${path}`;
  }

  const canCreate = $derived(plan !== null && !nameTaken && !portUser && (hasPassword || password.length > 0));
</script>

<section id="wizard" class="mx-auto max-w-3xl" aria-labelledby="wizard-heading">
  <header class="screen-heading">
    <h2 id="wizard-heading" tabindex="-1">Create a Drupal project</h2>
    <p>Configure the local environment, then review the generated DDEV project before creating it.</p>
  </header>
  {#if presets.length}
    <div class="mb-4 flex items-center gap-2 text-sm">
      <label for="preset" class="help">Start from preset</label>
      <select id="preset" class="input w-auto" value={presetName} onchange={(e) => applyPreset(e.currentTarget.value)}>
        <option value="">None</option>
        {#each presets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
      </select>
      {#if presetNote}<span class="text-xs text-[var(--color-warning)]">{presetNote}</span>{/if}
    </div>
  {/if}
  <ol class="mb-5 grid grid-cols-2 gap-2 sm:flex" aria-label="Site setup progress">
    {#each STEPS as label, i (label)}
      <li class="flex-auto">
        <button
          class="choice-tile min-h-11 w-full px-2.5 py-2 text-left text-sm! {i === stepIndex ? 'is-selected' : ''}"
          aria-current={i === stepIndex ? "step" : undefined}
          onclick={() => goTo(i)}>{i + 1}. {label}</button
        >
      </li>
    {/each}
  </ol>

  <div class="card space-y-4">
    {#if STEPS[stepIndex] === "Project"}
      <label class="field">
        <span>Project name</span>
        <input class="input" bind:value={displayName} placeholder="My new site" aria-invalid={errorFor("name") || nameTaken ? "true" : undefined} aria-describedby={[input.name ? "project-hostname" : "", nameTaken ? "project-name-taken" : "", errorFor("name") ? "project-name-error" : ""].filter(Boolean).join(" ") || undefined} />
        {#if input.name}
          <small id="project-hostname" class="help">https://{input.name}.ddev.site</small>
        {/if}
        {#if nameTaken}<small id="project-name-taken" class="error">A DDEV project named {input.name} already exists.</small>{/if}
        {#if errorFor("name")}<small id="project-name-error" class="error">{errorFor("name")}</small>{/if}
      </label>
      <label class="field">
        <span>Site name</span>
        <input class="input" bind:value={input.siteName} placeholder={displayName || "My Site"} aria-invalid={errorFor("siteName") ? "true" : undefined} aria-describedby={errorFor("siteName") ? "site-name-error" : undefined} />
        {#if errorFor("siteName")}<small id="site-name-error" class="error">{errorFor("siteName")}</small>{/if}
      </label>
      <div class="field">
        <label for="project-location">Location</label>
        <div class="flex flex-col gap-2 sm:flex-row">
          <input id="project-location" class="input flex-1" bind:value={input.parentDir} aria-invalid={errorFor("parentDir") ? "true" : undefined} aria-describedby={errorFor("parentDir") ? "project-location-help project-location-error" : "project-location-help"} />
          <button class="btn-ghost" onclick={pickParent}>Choose…</button>
        </div>
        <small id="project-location-help" class="help">Creates {input.parentDir}/{input.name || "…"}</small>
        {#if errorFor("parentDir")}<small id="project-location-error" class="error">{errorFor("parentDir")}</small>{/if}
      </div>
      <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <label class="field">
          <span>Admin username</span><input class="input" bind:value={input.adminName} aria-invalid={errorFor("adminName") ? "true" : undefined} aria-describedby={errorFor("adminName") ? "admin-name-error" : undefined} />
          {#if errorFor("adminName")}<small id="admin-name-error" class="error">{errorFor("adminName")}</small>{/if}
        </label>
        <label class="field">
          <span>Admin email</span><input class="input" bind:value={input.adminEmail} aria-invalid={errorFor("adminEmail") ? "true" : undefined} aria-describedby={errorFor("adminEmail") ? "admin-email-error" : undefined} />
          {#if errorFor("adminEmail")}<small id="admin-email-error" class="error">{errorFor("adminEmail")}</small>{/if}
        </label>
      </div>
      <div class="field">
        <label for="admin-password">Admin password for this project</label>
        <PasswordInput
          id="admin-password"
          bind:value={password}
          placeholder={hasPassword ? "Leave empty to use the saved password" : "Required (no saved password)"}
        />
      </div>
    {:else if STEPS[stepIndex] === "Drupal and PHP"}
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {#each compat.drupal as r (r.id)}
          <button
            class="choice-tile {input.drupal === r.id ? 'is-selected' : ''}"
            aria-pressed={input.drupal === r.id}
            onclick={() => chooseRelease(r.id)}
          >
            <div class="font-semibold">{r.label}</div>
            <div class="choice-detail font-mono">{r.package}</div>
            {#if r.status === "ending-soon"}
              <div class="mt-2 inline-block rounded bg-[var(--color-recessed)] px-2 py-0.5 text-xs text-[var(--color-warning)]">Ending soon</div>
            {:else if r.status === "preview"}
              <div class="mt-2 inline-block rounded bg-[var(--color-recessed)] px-2 py-0.5 text-xs text-[var(--color-primary-strong)]">Preview</div>
            {/if}
          </button>
        {/each}
      </div>
      {#if release.statusNote}<p class="text-sm text-[var(--color-warning)]">{release.statusNote}</p>{/if}
      <label class="field">
        <span>PHP version</span>
        <select class="input" bind:value={input.php}>
          {#each release.php as v (v)}<option value={v}>PHP {v}</option>{/each}
        </select>
      </label>
    {:else if STEPS[stepIndex] === "Environment"}
      <div class="field">
        <span>Web server</span>
        <div class="flex gap-2">
          {#each [["nginx-fpm", "Nginx"], ["apache-fpm", "Apache"]] as [value, label] (value)}
            <label class="flex items-center gap-2 text-sm">
              <input type="radio" bind:group={input.webserver} {value} /> {label}
            </label>
          {/each}
        </div>
      </div>
      <label class="field">
        <span>HTTPS port (optional)</span>
        <input class="input" type="number" min="1024" max="65535" step="1" placeholder="Use the shared router port" bind:value={input.httpsPort} aria-invalid={portUser || errorFor("httpsPort") ? "true" : undefined} aria-describedby={["https-port-help", portUser ? "https-port-used" : "", errorFor("httpsPort") ? "https-port-error" : ""].filter(Boolean).join(" ")} />
        <small id="https-port-help" class="help">
          Use a port from 30000 to 30500. The site opens at https://{input.name || "NAME"}.ddev.site:{input.httpsPort ?? "PORT"}.
        </small>
        {#if portUser}<small id="https-port-used" class="error">Port {input.httpsPort} is already used by {portUser}.</small>{/if}
        {#if errorFor("httpsPort")}<small id="https-port-error" class="error">{errorFor("httpsPort")}</small>{/if}
      </label>
      <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <label class="field">
          <span>Database</span>
          <select class="input" value={input.database.type} onchange={(e) => chooseDbType(e.currentTarget.value as DbType)} aria-invalid={errorFor("database") ? "true" : undefined} aria-describedby={errorFor("database") ? "database-error" : undefined}>
            {#each dbTypes as t (t)}<option value={t}>{DB_LABEL[t]}</option>{/each}
          </select>
        </label>
        <label class="field">
          <span>Version</span>
          <select class="input" bind:value={input.database.version}>
            {#each release.databases[input.database.type] ?? [] as v (v)}<option value={v}>{v}</option>{/each}
          </select>
        </label>
      </div>
      {#if input.database.type === "postgres"}
        <p class="text-sm text-[var(--color-warning)]">Some contributed modules don't fully support PostgreSQL.</p>
      {/if}
      {#if errorFor("database")}<p id="database-error" class="error">{errorFor("database")}</p>{/if}
      <label class="field">
        <span>Config sync directory (relative to web/)</span>
        <input class="input" bind:value={input.configSyncDir} aria-invalid={errorFor("configSyncDir") ? "true" : undefined} aria-describedby={errorFor("configSyncDir") ? "config-sync-error" : undefined} />
        {#if errorFor("configSyncDir")}<small id="config-sync-error" class="error">{errorFor("configSyncDir")}</small>{/if}
      </label>
    {:else if STEPS[stepIndex] === "Services"}
      <div class="flex items-center justify-between">
        <span class="text-sm font-medium">Services</span>
        <button class="text-xs text-[var(--color-primary-strong)] hover:underline" onclick={() => (input.services = [...(activePreset?.services ?? settings.services)])}>
          Reset to preset
        </button>
      </div>
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {#each compat.services as s (s.id)}
          {@const on = input.services.includes(s.id)}
          <button
            class="choice-tile flex min-h-11 items-center justify-between px-3 py-2 text-sm {on ? 'is-selected' : ''}"
            aria-pressed={on}
            onclick={() => toggleService(s.id)}
          >
            <span class="font-medium">{s.label}</span>
            <span class="text-xs text-[var(--color-muted)]">
              {s.wiring.kind === "module" ? "Wired into Drupal" : s.wiring.kind === "containerOnly" ? "Container only" : ""}
              {on ? " ✓" : ""}
            </span>
          </button>
        {/each}
      </div>
      {#if bundles.length}
        <div class="space-y-2">
          <span class="text-sm font-medium">Module bundles</span>
          <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {#each bundles as b (b.id)}
              {@const on = input.bundles.includes(b.id)}
              <button
                class="choice-tile {on ? 'is-selected' : ''}"
                aria-pressed={on}
                aria-describedby={errorFor("bundles") ? "bundles-error" : undefined}
                onclick={() => toggle("bundles", b.id)}
              >
                <span class="font-medium">{b.label}{on ? " ✓" : ""}</span>
                <span class="choice-detail">{b.description}</span>
              </button>
            {/each}
          </div>
          {#if errorFor("bundles")}<p id="bundles-error" class="error">{errorFor("bundles")}</p>{/if}
        </div>
      {/if}
      {#if recipes.length}
        <div class="space-y-2">
          <span class="text-sm font-medium">Recipes</span>
          <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {#each recipes as r (r.id)}
              {@const on = input.recipes.includes(r.id)}
              <button
                class="choice-tile {on ? 'is-selected' : ''}"
                aria-pressed={on}
                aria-describedby={errorFor("recipes") ? "recipes-error" : undefined}
                onclick={() => toggle("recipes", r.id)}
              >
                <span class="font-medium">{r.label}{on ? " ✓" : ""}</span>
                <span class="choice-detail">{r.description}</span>
              </button>
            {/each}
          </div>
          {#if errorFor("recipes")}<p id="recipes-error" class="error">{errorFor("recipes")}</p>{/if}
        </div>
      {:else}
        <p class="help text-sm">Recipes need Drupal 11 or Drupal CMS.</p>
      {/if}
      <label class="field">
        <span>Extra modules to enable</span>
        <input class="input" value={extraText} oninput={(e) => setExtraModules(e.currentTarget.value)} placeholder="media, admin_toolbar_search" aria-invalid={errorFor("extraModules") ? "true" : undefined} aria-describedby={errorFor("extraModules") ? "extra-modules-help extra-modules-error" : "extra-modules-help"} />
        <small id="extra-modules-help" class="help">
          Machine names of modules already in the codebase, such as core modules or ones from a bundle, separated by commas. They're
          enabled after install; nothing is downloaded.
        </small>
        {#if errorFor("extraModules")}<small id="extra-modules-error" class="error">{errorFor("extraModules")}</small>{/if}
      </label>
      <label class="field">
        <span>CI pipeline</span>
        <select class="input" bind:value={input.ci}>
          {#each CI_OPTIONS as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
        {#if input.ci === "gitlab"}
          <small class="help">Runs lint, analyse and unit tests in a PHP image. Kernel and functional tests stay local.</small>
        {:else if input.ci === "github"}
          <small class="help">Runs ddev composer lint, analyse and test with DDEV's GitHub action.</small>
        {/if}
      </label>
      <p class="text-xs help">
        Always included: drupal/core-dev with PHPUnit, PHPCS (Drupal and DrupalPractice) and PHPStan, plus
        <code>composer lint</code>, <code>composer analyse</code> and <code>composer test</code>.
      </p>
      <label class="flex items-center gap-2 text-sm">
        <input type="checkbox" bind:checked={input.exportConfigOnInstall} /> Export config after install
      </label>
      <label class="flex items-center gap-2 text-sm">
        <input type="checkbox" bind:checked={input.initGit} /> Initialize a Git repository
      </label>
    {:else if plan}
      <dl class="grid grid-cols-[minmax(6rem,10rem)_minmax(0,1fr)] gap-y-2 text-sm">
        <dt class="help">Folder</dt><dd class="font-mono">{plan.dir}</dd>
        <dt class="help">Hostname</dt><dd class="font-mono">{input.name}.ddev.site</dd>
        {#if input.httpsPort != null}
          <dt class="help">URL</dt><dd class="font-mono">https://{input.name}.ddev.site:{input.httpsPort}</dd>
        {/if}
        <dt class="help">Drupal</dt><dd>{release.label}, PHP {input.php}</dd>
        <dt class="help">Stack</dt><dd>{input.webserver}, {DB_LABEL[input.database.type]} {input.database.version}</dd>
        {#if input.bundles.length || input.recipes.length}
          <dt class="help">Add-ons</dt>
          <dd>
            {[
              ...compat.bundles.filter((b) => input.bundles.includes(b.id)).map((b) => b.label),
              ...compat.recipes.filter((r) => input.recipes.includes(r.id)).map((r) => `${r.label} recipe`),
            ].join(", ")}
          </dd>
        {/if}
        {#if input.extraModules.length}
          <dt class="help">Extra modules</dt><dd class="font-mono">{input.extraModules.join(", ")}</dd>
        {/if}
        {#if input.ci !== "none"}
          <dt class="help">CI</dt><dd>{CI_OPTIONS.find(([v]) => v === input.ci)?.[1]}</dd>
        {/if}
        <dt class="help">Steps</dt><dd>{plan.steps.length}</dd>
      </dl>
      {#if plan.folderWarning}<p class="text-sm text-[var(--color-warning)]">{plan.folderWarning}</p>{/if}
      {#if nameTaken}<p class="error">A DDEV project named {input.name} already exists.</p>{/if}
      {#if portUser}<p class="error">Port {input.httpsPort} is already used by {portUser}.</p>{/if}
      {#if !hasPassword && !password}<p class="error">Enter an admin password on the Project step.</p>{/if}
      <div class="flex flex-col gap-2 sm:flex-row">
        <button class="btn-ghost" onclick={() => (showScript = !showScript)}>
          {showScript ? "Hide" : "Show"} commands
        </button>
        <button class="btn-ghost" onclick={exportScript}>Export as script…</button>
      </div>
      <div class="flex flex-col gap-2 sm:flex-row">
        <input id="preset-name" class="input flex-1" bind:value={newPresetName} placeholder="Name these choices to reuse them" aria-label="Preset name" />
        <button class="btn-ghost" disabled={!newPresetName.trim()} onclick={savePreset}>Save as preset</button>
      </div>
      {#if notice}<p class="text-xs status-ok">{notice}</p>{/if}
      {#if showScript}
        <pre class="max-h-80 overflow-auto rounded-md terminal p-3 text-xs text-[var(--color-terminal-ink)]">{plan.script}</pre>
      {/if}
    {:else}
      <p class="text-sm help">Fix the highlighted fields to continue.</p>
    {/if}
  </div>

  <div class="mt-4 flex justify-between">
    <button class="btn-ghost" onclick={() => goTo(stepIndex - 1)} disabled={stepIndex === 0}>Back</button>
    {#if STEPS[stepIndex] === "Review"}
      <div class="flex items-center gap-3">
        {#if runInProgress}<span class="help text-sm">Another setup is running. Start this one when it finishes.</span>{/if}
        <button class="btn-primary" disabled={!canCreate || runInProgress} onclick={() => onCreate($state.snapshot(input), password || null)}>
          Create site
        </button>
      </div>
    {:else}
      <button class="btn-primary" onclick={() => goTo(stepIndex + 1)}>Next</button>
    {/if}
  </div>
</section>
