<script lang="ts">
  import { api, type CheckResult } from "$lib/api";

  let { projectsDir, onContinue }: { projectsDir: string; onContinue: () => void } = $props();

  let checks = $state<CheckResult[] | null>(null);
  const blocked = $derived(checks?.some((c) => c.level === "fail") ?? true);

  async function run() {
    checks = null;
    checks = await api.preflight(projectsDir);
  }

  $effect(() => {
    run();
  });

  const icon = { ok: "✓", warn: "!", fail: "✕" };
  const tone = {
    ok: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300",
    warn: "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300",
    fail: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300",
  };
</script>

<section class="card mx-auto max-w-2xl">
  <h2 class="text-lg font-semibold">Preflight</h2>
  <p class="mt-1 text-sm text-slate-500">Checking that this machine can run DDEV.</p>

  <ul class="mt-4 divide-y divide-slate-100 dark:divide-slate-800">
    {#if checks === null}
      <li class="py-3 text-sm text-slate-500">Running checks…</li>
    {:else}
      {#each checks as c (c.id)}
        <li class="flex items-start gap-3 py-3">
          <span class="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full text-xs font-bold {tone[c.level]}">
            {icon[c.level]}
          </span>
          <div class="text-sm">
            <div class="font-medium">{c.label}</div>
            <div class="text-slate-500">{c.detail}</div>
          </div>
        </li>
      {/each}
    {/if}
  </ul>

  <div class="mt-5 flex justify-end gap-2">
    <button class="btn-ghost" onclick={run} disabled={checks === null}>Check again</button>
    <button class="btn-primary" onclick={onContinue} disabled={blocked}>Continue</button>
  </div>
</section>
