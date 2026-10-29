<script lang="ts">
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, type Job, type Phase, type Plan, type RunEvent, type StepStatus } from "$lib/api";

  let {
    job,
    password,
    onBack,
    phase = $bindable(),
  }: { job: Job; password: string | null; onBack: () => void; phase?: Phase } = $props();

  const name = $derived(job.input.name);
  const verb = $derived(job.kind === "clone" ? "Cloning" : "Creating");

  const MAX_LINES = 4000;

  let plan = $state<Plan | null>(null);
  let statuses = $state<Record<string, StepStatus>>({});
  let lines = $state<{ id: string; line: string }[]>([]);
  phase = "running";
  let logPath = $state("");
  let error = $state("");
  let logEl: HTMLTextAreaElement | undefined = $state();

  const failed = $derived(
    Object.entries(statuses).find(([, s]) => s.status === "failed") as
      | [string, Extract<StepStatus, { status: "failed" }>]
      | undefined,
  );
  const doneCount = $derived(
    Object.values(statuses).filter((s) => s.status === "done" || s.status === "skipped").length,
  );
  const logOutput = $derived(lines.map((line) => `${line.id.padEnd(16).slice(0, 16)} ${line.line}`).join("\n"));

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

  async function deleteSite() {
    const ok = await ask(`Delete the DDEV project ${name} and remove ${plan?.dir}? No database snapshot is kept.`, {
      title: "Delete site",
      kind: "warning",
    });
    if (!ok) return;
    await act(async () => {
      await api.deleteProject(job);
      onBack();
    });
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

  const presentation: Record<StepStatus["status"], { icon: string; tone: string; label: string }> = {
    running: { icon: "◐", tone: "text-[var(--color-primary)]", label: "Running" },
    failed: { icon: "✕", tone: "status-danger", label: "Failed" },
    skipped: { icon: "↷", tone: "status-warning", label: "Skipped" },
    done: { icon: "✓", tone: "status-ok", label: "Done" },
  };
  const stepPresentation = (status?: StepStatus) => status ? presentation[status.status] : { icon: "○", tone: "text-[var(--color-border)]", label: "Waiting" };
</script>

<section class="grid gap-4 lg:grid-cols-[18rem_minmax(0,1fr)]" aria-labelledby="run-heading">
  <aside class="card max-h-72 overflow-auto p-3 lg:max-h-none">
    <div class="mb-2 px-2 text-sm text-[var(--color-muted)]" aria-label="Setup progress">
      {doneCount} of {plan?.steps.length ?? "…"} steps
    </div>
    <ol class="space-y-0.5 text-sm">
      {#each plan?.steps ?? [] as s (s.id)}
        <li class="flex gap-2 rounded px-2 py-2 {statuses[s.id]?.status === 'running' ? 'bg-[var(--color-recessed)]' : ''}">
          <span class="w-4 shrink-0 text-center {stepPresentation(statuses[s.id]).tone}" aria-hidden="true">{stepPresentation(statuses[s.id]).icon}</span>
          <span class="truncate" title={s.title}>{s.title}</span>
          <span class="sr-only">{stepPresentation(statuses[s.id]).label}</span>
        </li>
      {/each}
    </ol>
  </aside>

  <div class="flex min-h-0 flex-col gap-3">
    <div class="card flex flex-wrap items-center justify-between gap-3 py-3" aria-busy={phase === "running"}>
      <div class="min-w-0 text-sm">
        <h2 id="run-heading" class="sr-only">Project setup</h2>
        <div role="status" aria-live="polite" aria-atomic="true">
        {#if phase === "running"}
          <span class="font-medium">{verb} {name}…</span>
        {:else if phase === "done"}
          <span class="font-medium status-ok">{name} is ready.</span>
        {:else if phase === "cancelled"}
          <span class="font-medium status-warning">Cancelled.</span>
        {:else}
          <span class="font-medium status-danger">Failed{failed ? `: ${failed[1].message}` : ""}</span>
          {#if failed?.[1].retry.kind === "startOver"}
            <div class="text-xs help">{failed[1].retry.reason}</div>
          {/if}
        {/if}
        </div>
        {#if error}<div class="error" role="alert">{error}</div>{/if}
        {#if logPath && phase !== "running"}<div class="truncate text-xs text-[var(--color-muted)]">Log: {logPath}</div>{/if}
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
          <button class="btn-danger" onclick={deleteSite}>Delete site</button>
        {:else}
          {#if failed?.[1].retry.kind !== "startOver"}
            <button class="btn-primary" onclick={() => start(true)}>{phase === "cancelled" ? "Resume" : "Retry"}</button>
          {/if}
          <button class="btn-danger" onclick={deleteAndStartOver}>Delete and start over</button>
          <button class="btn-ghost" onclick={onBack}>Back</button>
        {/if}
      </div>
    </div>

    <textarea
      bind:this={logEl}
      readonly
      aria-label="Setup output"
      aria-live="off"
      class="terminal min-h-72 flex-1 resize-none p-4 font-mono text-xs leading-relaxed"
      value={logOutput}></textarea>
  </div>
</section>
