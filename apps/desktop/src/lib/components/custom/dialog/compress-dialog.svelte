<script lang="ts">
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import DialogButton from "./dialog-button.svelte";
  import DialogShell from "./dialog-shell.svelte";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/index.js";
  import {
    availableCompressFormats,
    compressExtension,
    defaultArchiveName,
    pathExists,
    type CompressFormat,
  } from "$lib/file-ops/archive.js";
  import { parentPath } from "$lib/file-ops/files.js";

  let {
    open = $bindable(false),
    targets,
    sevenZipPath = null,
    onCreate,
  }: {
    open?: boolean;
    targets: DirectoryEntry[];
    sevenZipPath?: string | null;
    onCreate: (name: string, format: CompressFormat) => void;
  } = $props();

  let input = $state<HTMLInputElement | null>(null);
  let name = $state("");
  let format = $state<CompressFormat>("zip");
  let exists = $state(false);
  let checking = $state(false);
  let wasOpen = $state(false);

  const formats = $derived(availableCompressFormats(sevenZipPath));
  const trimmed = $derived(name.trim());
  const invalidName = $derived(trimmed.length === 0 || /[\\/]/.test(trimmed));
  const extension = $derived(compressExtension(format));
  const destination = $derived.by(() => {
    if (targets.length === 0 || invalidName) return "";
    const parent = parentPath(targets[0].path) || targets[0].path;
    return `${parent}${trimmed}${extension}`;
  });
  const conflict = $derived(!invalidName && exists);
  const valid = $derived(!invalidName && !conflict && !checking && targets.length > 0);

  $effect(() => {
    if (open && !wasOpen) {
      name = defaultArchiveName(targets.map((target) => target.name));
      format = availableCompressFormats(sevenZipPath)[0]?.value ?? "zip";
      exists = false;
    }
    wasOpen = open;
  });

  $effect(() => {
    const path = destination;
    if (!open || !path) {
      exists = false;
      checking = false;
      return;
    }
    checking = true;
    let cancelled = false;
    const timer = setTimeout(() => {
      pathExists(path)
        .then((value) => {
          if (!cancelled) exists = value;
        })
        .catch(() => {
          if (!cancelled) exists = false;
        })
        .finally(() => {
          if (!cancelled) checking = false;
        });
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!valid) return;
    onCreate(trimmed, format);
  }
</script>

<DialogShell bind:open title={targets.length === 1 ? "Compress item" : `Compress ${targets.length} items`} initialFocus={input}>
  <form class="form" id="compress-form" onsubmit={submit}>
    <label for="compress-name">Name</label>
    <div class="name-row">
      <input
        id="compress-name"
        bind:this={input}
        bind:value={name}
        class:invalid={conflict}
        placeholder="Archive"
        spellcheck="false"
        autocomplete="off" />
      <span class="extension">{extension}</span>
    </div>

    <label for="compress-format">Format</label>
    <select id="compress-format" bind:value={format}>
      {#each formats as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>

    {#if conflict}
      <p class="error" role="alert">
        <CircleAlertIcon />
        “{trimmed}{extension}” already exists here.
      </p>
    {:else}
      <p class="help">
        {targets.length === 1 ? "Saved next to the item." : "Saved in the current folder."}
      </p>
    {/if}
  </form>
  {#snippet footer()}
    <DialogButton onclick={() => (open = false)}>Cancel</DialogButton>
    <DialogButton type="submit" form="compress-form" variant="primary" disabled={!valid}>
      {checking ? "Checking…" : "Create"}
    </DialogButton>
  {/snippet}
</DialogShell>

<style>
  .form {
    display: grid;
    margin-top: 6px;
  }
  label {
    margin-bottom: 6px;
    color: #a8a4a1;
    font-size: 11.5px;
    font-weight: 500;
  }
  label:not(:first-child) {
    margin-top: 14px;
  }
  .name-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input,
  select {
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    outline: none;
    background: #201e1d;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
    color: #eceae8;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
    transition: box-shadow 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  select {
    font-family: inherit;
    appearance: none;
  }
  input::placeholder {
    color: #8a8683;
  }
  input:focus,
  select:focus {
    box-shadow:
      inset 0 0 0 1px rgb(10 155 255 / 0.9),
      0 0 0 3px rgb(10 155 255 / 0.18);
  }
  input.invalid {
    box-shadow:
      inset 0 0 0 1px rgb(255 107 94 / 0.9),
      0 0 0 3px rgb(255 107 94 / 0.18);
  }
  .extension {
    flex-shrink: 0;
    color: #8f8b88;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .help {
    margin: 8px 0 0;
    color: #8f8b88;
    font-size: 11px;
    line-height: 1.4;
  }
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 8px 0 0;
    color: #ff6b5e;
    font-size: 11.5px;
    line-height: 1.4;
  }
  .error :global(svg) {
    flex-shrink: 0;
    width: 13px;
    height: 13px;
    margin-top: 1px;
  }
</style>
