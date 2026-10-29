<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import PasswordInput from "./PasswordInput.svelte";
  import { api, fieldErrors, type AppSettings, type CloneInput, type Compat, type FieldError, type Job } from "$lib/api";

  let {
    compat,
    settings,
    hasPassword,
    onStart,
    onCancel,
    runInProgress,
  }: {
    compat: Compat;
    settings: AppSettings;
    hasPassword: boolean;
    runInProgress: boolean;
    onStart: (job: Job, password: string | null) => void;
    onCancel: () => void;
  } = $props();

  type Source = "existingConfig" | "dump" | "none";
  const SOURCES: [Source, string, string][] = [
    ["existingConfig", "Install from the repo's config", "drush site:install --existing-config, using the exported config in the repo."],
    ["dump", "Import a database dump", "ddev import-db from a .sql, .sql.gz or similar file on this computer."],
    ["none", "No database yet", "Clone and start DDEV only. Import a database yourself later."],
  ];

  const defaults = untrack(() => $state.snapshot(settings));
  const firstRelease = untrack(() => compat.drupal.find((r) => r.status === "supported") ?? compat.drupal[0]);

  let repoUrl = $state("");
  let name = $state("");
  let nameEdited = $state(false);
  let parentDir = $state(defaults.projectsDir);
  let drupal = $state(firstRelease.id);
  let source = $state<Source>("existingConfig");
  let dumpPath = $state("");
  let adminName = $state(defaults.adminName);
  let adminEmail = $state(defaults.adminEmail);
  let password = $state("");
  let errors = $state<FieldError[]>([]);
  let taken = $state<string[]>([]);
  let busy = $state(false);

  api.existingProjects().then((names) => (taken = names));

  $effect(() => {
    const url = repoUrl;
    if (!nameEdited) api.suggestedCloneName(url).then((n) => (name = n));
  });

  const input = $derived<CloneInput>({
    repoUrl,
    parentDir,
    name,
    drupal,
    database: source === "dump" ? { kind: "dump", path: dumpPath } : { kind: source },
    adminName,
    adminEmail,
  });
  const nameTaken = $derived(taken.includes(name));
  const needsPassword = $derived(source === "existingConfig" && !hasPassword && !password);

  const errorFor = (field: FieldError["field"]) => errors.find((e) => e.field === field)?.message;

  async function pickParent() {
    const dir = await open({ directory: true, defaultPath: parentDir });
    if (typeof dir === "string") parentDir = dir;
  }

  async function pickDump() {
    const file = await open({ filters: [{ name: "Database dump", extensions: ["sql", "gz", "bz2", "xz", "zip", "tgz", "tar"] }] });
    if (typeof file === "string") dumpPath = file;
  }

  async function start() {
    busy = true;
    const job: Job = { kind: "clone", input: $state.snapshot(input) as CloneInput };
    try {
      await api.plan(job);
      errors = [];
      onStart(job, password || null);
    } catch (e) {
      errors = fieldErrors(e);
      if (!errors.length) errors = [{ field: "repoUrl", message: String(e) }];
    }
    busy = false;
  }
</script>

<section class="mx-auto max-w-3xl space-y-4" aria-busy={busy} aria-labelledby="clone-heading">
  <header class="screen-heading">
    <h2 id="clone-heading">Clone a project from Git</h2>
    <p>Clone the repository, start DDEV, run Composer, and prepare the database.</p>
  </header>

  <div class="card space-y-4">
    <label class="field">
      <span>Repository URL</span>
      <input class="input font-mono" bind:value={repoUrl} placeholder="git@github.com:acme/intranet.git" aria-invalid={errorFor("repoUrl") ? "true" : undefined} aria-describedby={errorFor("repoUrl") ? "clone-repo-error" : undefined} />
      {#if errorFor("repoUrl")}<small id="clone-repo-error" class="error">{errorFor("repoUrl")}</small>{/if}
    </label>
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <label class="field">
        <span>Project name</span>
        <input class="input" value={name} oninput={(e) => ((name = e.currentTarget.value), (nameEdited = true))} aria-invalid={nameTaken || errorFor("name") ? "true" : undefined} aria-describedby={[name ? "clone-hostname" : "", nameTaken ? "clone-name-taken" : "", errorFor("name") ? "clone-name-error" : ""].filter(Boolean).join(" ") || undefined} />
        {#if name}<small id="clone-hostname" class="help">https://{name}.ddev.site</small>{/if}
        {#if nameTaken}<small id="clone-name-taken" class="error">A DDEV project named {name} already exists.</small>{/if}
        {#if errorFor("name")}<small id="clone-name-error" class="error">{errorFor("name")}</small>{/if}
      </label>
      <label class="field">
        <span>Drupal version, if the repo has no DDEV config</span>
        <select class="input" bind:value={drupal}>
          {#each compat.drupal as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
        </select>
      </label>
    </div>
    <div class="field">
      <label for="clone-location">Location</label>
      <div class="flex flex-col gap-2 sm:flex-row">
        <input id="clone-location" class="input flex-1" bind:value={parentDir} aria-describedby={errorFor("parentDir") ? "clone-location-help clone-location-error" : "clone-location-help"} aria-invalid={errorFor("parentDir") ? "true" : undefined} />
        <button class="btn-ghost" onclick={pickParent}>Choose…</button>
      </div>
      <small id="clone-location-help" class="help">Clones into {parentDir}/{name || "…"}</small>
      {#if errorFor("parentDir")}<small id="clone-location-error" class="error">{errorFor("parentDir")}</small>{/if}
    </div>

    <fieldset class="space-y-2" aria-describedby={errorFor("database") ? "clone-database-error" : undefined}>
      <legend class="text-sm font-medium">Database</legend>
      {#each SOURCES as [value, label, help] (value)}
        <label class="flex items-start gap-2 text-sm">
          <input type="radio" class="mt-1" bind:group={source} {value} />
          <span><span class="font-medium">{label}</span><span class="block text-xs help">{help}</span></span>
        </label>
      {/each}
      {#if source === "dump"}
        <div class="flex flex-col gap-2 pl-6 sm:flex-row">
          <input id="clone-dump-path" class="input flex-1 font-mono" bind:value={dumpPath} placeholder="/path/to/db.sql.gz" aria-label="Database dump path" />
          <button class="btn-ghost" onclick={pickDump}>Choose…</button>
        </div>
      {/if}
      {#if errorFor("database")}<p id="clone-database-error" class="error">{errorFor("database")}</p>{/if}
    </fieldset>

    {#if source === "existingConfig"}
      <div class="grid grid-cols-2 gap-4">
        <label class="field">
          <span>Admin username</span><input class="input" bind:value={adminName} aria-invalid={errorFor("adminName") ? "true" : undefined} aria-describedby={errorFor("adminName") ? "clone-admin-name-error" : undefined} />
          {#if errorFor("adminName")}<small id="clone-admin-name-error" class="error">{errorFor("adminName")}</small>{/if}
        </label>
        <label class="field">
          <span>Admin email</span><input class="input" bind:value={adminEmail} aria-invalid={errorFor("adminEmail") ? "true" : undefined} aria-describedby={errorFor("adminEmail") ? "clone-admin-email-error" : undefined} />
          {#if errorFor("adminEmail")}<small id="clone-admin-email-error" class="error">{errorFor("adminEmail")}</small>{/if}
        </label>
      </div>
      <div class="field">
        <label for="clone-password">Admin password</label>
        <PasswordInput
          id="clone-password"
          bind:value={password}
          placeholder={hasPassword ? "Leave empty to use the saved password" : "Required (no saved password)"}
        />
      </div>
    {/if}
  </div>

  <div class="flex justify-between">
    <button class="btn-ghost" onclick={onCancel}>Cancel</button>
    <div class="flex items-center gap-3">
      {#if runInProgress}<span class="help text-sm">Another setup is running. Start this one when it finishes.</span>{/if}
      <button class="btn-primary" disabled={busy || runInProgress || !repoUrl || !name || nameTaken || needsPassword} onclick={start}>Clone site</button>
    </div>
  </div>
</section>
