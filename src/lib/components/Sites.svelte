<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, type Site, type SiteAction } from "$lib/api";

  let { onNew }: { onNew: () => void } = $props();

  type Action = { id: SiteAction; label: string; done: string; running: boolean | null; confirm?: (s: Site) => string };
  const ACTIONS: Action[] = [
    { id: "start", label: "Start", done: "Started.", running: false },
    { id: "stop", label: "Stop", done: "Stopped.", running: true },
    { id: "snapshot", label: "Snapshot", done: "Database snapshot saved in .ddev/db_snapshots.", running: true },
    {
      id: "configImport",
      label: "Import config",
      done: "Config imported.",
      running: true,
      confirm: (s) => `Import config into ${s.name}? Active configuration is replaced by the sync folder.`,
    },
    { id: "configExport", label: "Export config", done: "Config exported to the sync folder.", running: true },
    {
      id: "delete",
      label: "Delete",
      done: "",
      running: null,
      confirm: (s) =>
        `Delete the DDEV project ${s.name}? DDEV snapshots the database first, then removes its containers and database. The files in ${s.dir} stay.`,
    },
  ];

  let sites = $state<Site[] | null>(null);
  let busy = $state<Record<string, SiteAction>>({});
  let notes = $state<Record<string, { ok: boolean; text: string }>>({});
  let loadError = $state("");

  async function load() {
    loadError = "";
    try {
      sites = await api.listSites();
    } catch (e) {
      loadError = String(e);
    }
  }

  async function run(site: Site, a: Action) {
    if (a.confirm && !(await ask(a.confirm(site), { title: a.label, kind: "warning" }))) return;
    busy[site.name] = a.id;
    delete notes[site.name];
    try {
      await api.siteAction(site.name, a.id);
      if (a.done) notes[site.name] = { ok: true, text: a.done };
    } catch (e) {
      notes[site.name] = { ok: false, text: String(e) };
    }
    delete busy[site.name];
    await load();
  }

  const enabled = (s: Site, a: Action) =>
    !busy[s.name] && (a.running === null || a.running === (s.status === "running"));

  load();
</script>

<section class="mx-auto max-w-4xl space-y-4" aria-labelledby="sites-heading" aria-busy={sites === null}>
  <div class="flex items-center justify-between">
    <div class="screen-heading mb-0">
      <h2 id="sites-heading">Sites</h2>
      <p>Every project DDEV knows about on this machine.</p>
    </div>
    <div class="flex gap-2">
      <button class="btn-ghost" onclick={load}>Refresh</button>
      <button class="btn-primary" onclick={onNew}>New site</button>
    </div>
  </div>

  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}

  {#if sites === null}
    <p class="help" role="status">Reading ddev list…</p>
  {:else if sites.length === 0}
    <div class="card help">DDEV has no projects yet.</div>
  {:else}
    <ul class="overflow-hidden rounded-2xl border border-[var(--color-divider)] bg-[var(--color-panel)]">
      {#each sites as s (s.name)}
        <li class="space-y-3 border-b border-[var(--color-divider)] p-4 last:border-b-0">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span
                  class="size-2 rounded-full {s.status === 'running' ? 'bg-[var(--color-success)]' : 'bg-[var(--color-border)]'}"
                  aria-hidden="true"
                ></span>
                <span class="font-medium">{s.name}</span>
                <span class="help text-xs">{s.kind} · {busy[s.name] ? "working…" : s.status}</span>
              </div>
              <button class="block max-w-full truncate text-left text-xs text-[var(--color-muted)] underline-offset-2 hover:underline" onclick={() => revealItemInDir(s.dir)}>
                {s.dir}
              </button>
            </div>
            {#if s.url && s.status === "running"}
              <button class="btn-ghost" onclick={() => s.url && openUrl(s.url)}>Open site</button>
            {/if}
          </div>
          <div class="flex flex-wrap gap-2">
            {#each ACTIONS as a (a.id)}
              <button
                class="{a.id === 'delete' ? 'btn-danger' : 'btn-ghost'} px-3 py-1.5 text-xs"
                disabled={!enabled(s, a)}
                onclick={() => run(s, a)}
              >
                {busy[s.name] === a.id ? `${a.label}…` : a.label}
              </button>
            {/each}
          </div>
          {#if notes[s.name]}
            <p
              class="whitespace-pre-wrap text-sm {notes[s.name].ok ? 'status-ok' : 'error'}"
              role={notes[s.name].ok ? "status" : "alert"}>{notes[s.name].text}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
