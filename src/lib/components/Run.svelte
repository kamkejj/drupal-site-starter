<script lang="ts">
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, type Job, type Plan, type RunEvent, type StepStatus } from "$lib/api";

  let {
    job,
    password,
    onBack,
  }: { job: Job; password: string | null; onBack: () => void } = $props();

  const name = $derived(job.input.name);
  const verb = $derived(job.kind === "clone" ? "Cloning" : "Creating");

  type Phase = "running" | "failed" | "cancelled" | "done";
  const MAX_LINES = 4000;

  let plan = $state<Plan | null>(null);
  let statuses = $state<Record<string, StepStatus>>({});
  let lines = $state<{ id: string; line: string }[]>([]);
  let phase = $state<Phase>("running");
  let logPath = $state("");
  let error = $state("");
  let logEl: HTMLPreElement | undefined = $state();

  const failed = $derived(
    Object.entries(statuses).find(([, s]) => s.status === "failed") as
      | [string, Extract<StepStatus, { status: "failed" }>]
      | undefined,
  );
  const doneCount = $derived(
    Object.values(statuses).filter((s) => s.status === "done" || s.status === "skipped").length,
  );

  function onEvent(e: RunEvent) {
    if (e.type === "log") {
      lines.push({ id: e.id, line: e.line });
      if (lines.length > MAX_LINES) lines.splice(0, lines.length - MAX_LINES);
      queueMicrotask(() => logEl?.scrollTo({ top: logEl.scrollHeight }));
    } else if (e.type === "step") {
      const { type: _, id, ...status } = e;
      statuses[id] = status as StepStatus;
    } else {
      phase = e.ok ? "done" : e.cancelled ? "cancelled" : "failed";
    }
  }

  async function start(resume: boolean) {
    phase = "running";
    error = "";
    if (!resume) statuses = {};
    else if (failed) delete statuses[failed[0]];
    try {
      logPath = await api.startRun(job, password, resume, onEvent);
    } catch (e) {
      error = String(e);
      phase = "failed";
    }
  }

  async function act(action: () => Promise<unknown>) {
    error = "";
    try {
      await action();
    } catch (e) {
      error = String(e);
    }
  }

  async function deleteAndStartOver() {
    const ok = await ask(`Delete the DDEV project ${name} and remove ${plan?.dir}?`, {
      title: "Delete and start over",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await api.deleteProject(job);
      lines = [];
      start(false);
    } catch (e) {
      error = String(e);
    }
  }

  untrack(() => api.plan(job)).then(
    (p) => {
      plan = p;
      start(false);
    },
    (e) => {
      error = Array.isArray(e) ? e.map((f) => f.message).join(" ") : String(e);
      phase = "failed";
    },
  );

  const icon = (s?: StepStatus) =>
    !s ? "○" : s.status === "running" ? "◐" : s.status === "failed" ? "✕" : s.status === "skipped" ? "↷" : "✓";
  const tone = (s?: StepStatus) =>
    !s
      ? "text-slate-400"
      : s.status === "running"
        ? "text-drop animate-pulse"
        : s.status === "failed"
          ? "text-red-600"
          : "text-emerald-600";
</script>

<section class="grid h-[calc(100vh-7rem)] grid-cols-[18rem_minmax(0,1fr)] gap-4">
  <aside class="card overflow-auto p-3">
    <div class="mb-2 px-2 text-xs text-slate-500">
      {doneCount} of {plan?.steps.length ?? "…"} steps
    </div>
    <ol class="space-y-0.5 text-sm">
      {#each plan?.steps ?? [] as s (s.id)}
        <li class="flex gap-2 rounded px-2 py-1 {statuses[s.id]?.status === 'running' ? 'bg-drop/10' : ''}">
          <span class="w-4 shrink-0 text-center {tone(statuses[s.id])}">{icon(statuses[s.id])}</span>
          <span class="truncate" title={s.title}>{s.title}</span>
        </li>
      {/each}
    </ol>
  </aside>

  <div class="flex min-h-0 flex-col gap-3">
    <div class="card flex flex-wrap items-center justify-between gap-3 py-3">
      <div class="min-w-0 text-sm">
        {#if phase === "running"}
          <span class="font-medium">{verb} {name}…</span>
        {:else if phase === "done"}
          <span class="font-medium text-emerald-700">{name} is ready.</span>
        {:else if phase === "cancelled"}
          <span class="font-medium text-amber-700">Cancelled.</span>
        {:else}
          <span class="font-medium text-red-700">Failed{failed ? `: ${failed[1].message}` : ""}</span>
          {#if failed?.[1].retry.kind === "startOver"}
            <div class="text-xs text-slate-500">{failed[1].retry.reason}</div>
          {/if}
        {/if}
        {#if error}<div class="error">{error}</div>{/if}
        {#if logPath && phase !== "running"}<div class="truncate text-xs text-slate-500">Log: {logPath}</div>{/if}
      </div>
      <div class="flex shrink-0 flex-wrap gap-2">
        {#if phase === "running"}
          <button class="btn-ghost" onclick={() => api.cancelRun()}>Cancel</button>
        {:else if phase === "done"}
          <button class="btn-ghost" onclick={() => plan && revealItemInDir(plan.dir)}>Show folder</button>
          <button class="btn-ghost" onclick={() => act(() => api.openInIde(job))}>Open in IDE</button>
          {#if job.kind === "create" || job.input.database.kind !== "none"}
            <button class="btn-ghost" onclick={() => act(async () => openUrl(await api.loginLink(job)))}>Log in as admin</button>
          {/if}
          <button class="btn-primary" onclick={() => act(async () => openUrl(await api.projectUrl(name)))}>Open site</button>
          <button class="btn-ghost" onclick={onBack}>New site</button>
        {:else}
          {#if failed?.[1].retry.kind !== "startOver"}
            <button class="btn-primary" onclick={() => start(true)}>{phase === "cancelled" ? "Resume" : "Retry"}</button>
          {/if}
          <button class="btn-danger" onclick={deleteAndStartOver}>Delete and start over</button>
          <button class="btn-ghost" onclick={onBack}>Back</button>
        {/if}
      </div>
    </div>

    <pre
      bind:this={logEl}
      class="min-h-0 flex-1 overflow-auto rounded-lg bg-slate-950 p-3 font-mono text-xs leading-relaxed text-slate-200">{#each lines as l}<span
          class="text-slate-500">{l.id.padEnd(16).slice(0, 16)} </span>{l.line}
{/each}</pre>
  </div>
</section>
