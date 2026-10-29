<script lang="ts">
  import { appearances, preferences, textSizes, themes, themesFor, type Mode } from "$lib/preferences.svelte";
  import { appSettings, modifier, shortcuts } from "$lib/shortcuts.svelte";

  let dialog: HTMLDialogElement;

  $effect(() => {
    if (appSettings.open && !dialog.open) dialog.showModal();
    if (!appSettings.open && dialog.open) dialog.close();
  });

  const themePickers: { mode: Mode; legend: string }[] = [
    { mode: "dark", legend: "Dark theme" },
    { mode: "light", legend: "Light theme" },
  ];
  const previewRoles = ["primary", "accent", "success", "warning", "danger"];
</script>

<dialog bind:this={dialog} class="app-settings" aria-labelledby="app-settings-heading" onclose={() => (appSettings.open = false)}>
  <header class="app-settings-header">
    <div>
      <h2 id="app-settings-heading">App settings</h2>
      <p class="help">How this app looks and behaves. Changes apply right away.</p>
    </div>
    <button class="btn-ghost" onclick={() => (appSettings.open = false)}>Done</button>
  </header>

  <div class="app-settings-body">
    <fieldset class="field">
      <legend>Appearance</legend>
      <div class="segmented">
        {#each Object.entries(appearances) as [value, label] (value)}
          <label><input type="radio" name="appearance" {value} bind:group={preferences.appearance} />{label}</label>
        {/each}
      </div>
    </fieldset>

    {#each themePickers as { mode, legend } (mode)}
      <fieldset class="field">
        <legend>{legend}</legend>
        <div class="theme-options">
          {#each themesFor(mode) as id (id)}
            <label class="theme-option">
              <input type="radio" name="theme-{mode}" value={id} bind:group={preferences.themes[mode]} />
              <span class="theme-preview" data-theme={id} aria-hidden="true">
                <span>Aa</span>
                {#each previewRoles as role (role)}<i style:background="var(--color-{role})"></i>{/each}
              </span>
              {themes[id].label}
            </label>
          {/each}
        </div>
      </fieldset>
    {/each}

    <fieldset class="field">
      <legend>Text size</legend>
      <div class="segmented">
        {#each Object.entries(textSizes) as [value, label] (value)}
          <label><input type="radio" name="text-size" {value} bind:group={preferences.textSize} />{label}</label>
        {/each}
      </div>
    </fieldset>

    <section class="field" aria-labelledby="shortcuts-heading">
      <h3 id="shortcuts-heading">Keyboard shortcuts</h3>
      <dl class="shortcut-list">
        {#each shortcuts as s (s.label)}
          <div><dt><kbd>{modifier.label}{s.label}</kbd></dt><dd>{s.description}</dd></div>
        {/each}
      </dl>
    </section>
  </div>
</dialog>
