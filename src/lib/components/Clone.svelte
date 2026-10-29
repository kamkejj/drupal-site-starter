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
  }: {
    compat: Compat;
    settings: AppSettings;
    hasPassword: boolean;
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

<section class="mx-auto max-w-3xl space-y-4">
  <div>
    <h2 class="text-lg font-semibold">Clone a project from Git</h2>
    <p class="text-sm text-slate-500">Clones the repository, starts it in DDEV, runs composer install and sets up the database.</p>
  </div>

  <div class="card space-y-4">
    <label class="field">
      <span>Repository URL</span>
      <input class="input font-mono" bind:value={repoUrl} placeholder="git@github.com:acme/intranet.git" />
      {#if errorFor("repoUrl")}<small class="error">{errorFor("repoUrl")}</small>{/if}
    </label>
    <div class="grid grid-cols-2 gap-4">
      <label class="field">
        <span>Project name</span>
        <input class="input" value={name} oninput={(e) => ((name = e.currentTarget.value), (nameEdited = true))} />
        {#if name}<small class="text-slate-500">https://{name}.ddev.site</small>{/if}
        {#if nameTaken}<small class="error">A DDEV project named {name} already exists.</small>{/if}
        {#if errorFor("name")}<small class="error">{errorFor("name")}</small>{/if}
      </label>
      <label class="field">
        <span>Drupal version, if the repo has no DDEV config</span>
        <select class="input" bind:value={drupal}>
          {#each compat.drupal as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
        </select>
      </label>
    </div>
    <div class="field">
      <span>Location</span>
      <div class="flex gap-2">
        <input class="input flex-1" bind:value={parentDir} />
        <button class="btn-ghost" onclick={pickParent}>Choose…</button>
      </div>
      <small class="text-slate-500">Clones into {parentDir}/{name || "…"}</small>
      {#if errorFor("parentDir")}<small class="error">{errorFor("parentDir")}</small>{/if}
    </div>

    <fieldset class="space-y-2">
      <legend class="text-sm font-medium">Database</legend>
      {#each SOURCES as [value, label, help] (value)}
        <label class="flex items-start gap-2 text-sm">
          <input type="radio" class="mt-1" bind:group={source} {value} />
          <span><span class="font-medium">{label}</span><span class="block text-xs text-slate-500">{help}</span></span>
        </label>
      {/each}
      {#if source === "dump"}
        <div class="flex gap-2 pl-6">
          <input class="input flex-1 font-mono" bind:value={dumpPath} placeholder="/path/to/db.sql.gz" />
          <button class="btn-ghost" onclick={pickDump}>Choose…</button>
        </div>
      {/if}
      {#if errorFor("database")}<p class="error">{errorFor("database")}</p>{/if}
    </fieldset>

    {#if source === "existingConfig"}
      <div class="grid grid-cols-2 gap-4">
        <label class="field">
          <span>Admin username</span><input class="input" bind:value={adminName} />
          {#if errorFor("adminName")}<small class="error">{errorFor("adminName")}</small>{/if}
        </label>
        <label class="field">
          <span>Admin email</span><input class="input" bind:value={adminEmail} />
          {#if errorFor("adminEmail")}<small class="error">{errorFor("adminEmail")}</small>{/if}
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
    <button class="btn-primary" disabled={busy || !repoUrl || !name || nameTaken || needsPassword} onclick={start}>Clone site</button>
  </div>
</section>
