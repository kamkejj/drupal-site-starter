<script lang="ts">
  let {
    id,
    value = $bindable(""),
    placeholder = "",
  }: { id: string; value?: string; placeholder?: string } = $props();

  let visible = $state(false);
  const label = $derived(visible ? "Hide password" : "Show password");
</script>

<div class="relative">
  <input
    {id}
    class="input w-full pr-10"
    type={visible ? "text" : "password"}
    autocomplete="new-password"
    bind:value
    {placeholder}
  />
  <button
    type="button"
    class="group absolute inset-y-0 right-0 grid w-10 place-items-center text-slate-400 outline-none hover:text-slate-700 focus-visible:text-drop dark:hover:text-slate-200"
    aria-label={label}
    aria-pressed={visible}
    onclick={() => (visible = !visible)}
  >
    <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {#if visible}
        <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" />
        <circle cx="12" cy="12" r="3" />
      {:else}
        <path d="M10.7 5.1A10.4 10.4 0 0 1 12 5c7 0 10 7 10 7a13.2 13.2 0 0 1-1.7 2.7" />
        <path d="M6.6 6.6A13.5 13.5 0 0 0 2 12s3 7 10 7a9.7 9.7 0 0 0 5.4-1.6" />
        <path d="M9.9 9.9a3 3 0 1 0 4.2 4.2" />
        <path d="m2 2 20 20" />
      {/if}
    </svg>
    <span
      class="pointer-events-none absolute right-0 bottom-full mb-1.5 rounded bg-slate-900 px-2 py-1 text-xs whitespace-nowrap text-white opacity-0 shadow transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100 dark:bg-slate-700"
      aria-hidden="true">{label}</span
    >
  </button>
</div>
