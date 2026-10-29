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

  const presentation: Record<CheckResult["level"], { icon: string; label: string; tone: string }> = {
    ok: { icon: "✓", label: "Passed", tone: "status-ok" },
    warn: { icon: "!", label: "Warning", tone: "status-warning" },
    fail: { icon: "✕", label: "Failed", tone: "status-danger" },
  };
</script>

<section class="mx-auto max-w-2xl" aria-busy={checks === null} aria-labelledby="preflight-heading">
  <header class="screen-heading">
    <h2 id="preflight-heading">Preflight</h2>
    <p>Checking that this machine can run DDEV.</p>
  </header>

  <div class="card">
  <p class="sr-only" role="status" aria-live="polite">
    {checks === null ? "Running environment checks." : blocked ? "Environment checks need attention." : "Environment checks passed."}
  </p>
  <ul class="divide-y divide-[var(--color-divider)]">
    {#if checks === null}
      <li class="py-3 text-sm help">Running checks…</li>
    {:else}
      {#each checks as c (c.id)}
        <li class="flex items-start gap-3 py-3">
          <span class="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full bg-[var(--color-recessed)] text-xs font-bold {presentation[c.level].tone}" aria-hidden="true">
            {presentation[c.level].icon}
          </span>
          <div class="text-sm">
            <div class="font-bold">{c.label} <span class="font-normal {presentation[c.level].tone}">{presentation[c.level].label}</span></div>
            <div class="help">{c.detail}</div>
          </div>
        </li>
      {/each}
    {/if}
  </ul>

  <div class="mt-5 flex justify-end gap-2">
    <button class="btn-ghost" onclick={run} disabled={checks === null}>Check again</button>
    <button class="btn-primary" onclick={onContinue} disabled={blocked}>Continue</button>
  </div>
  </div>
</section>
