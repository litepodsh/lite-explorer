<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { on } from "svelte/events";
  import type { Editor, TLPageId } from "tldraw";
  import type { TldrawPreview } from "./tldraw-viewer.js";

  let { path, name }: { path: string; name: string } = $props();
  let container = $state<HTMLDivElement>();
  let editor = $state.raw<Editor | null>(null);
  let currentPageId = $state("");
  let error = $state("");
  const pages = $derived(editor?.getPages() ?? []);

  $effect(() => {
    const target = path;
    const host = container;
    if (!host) return;
    const stopKeys = on(host, "keydown", (event) => {
      if (event.key !== "Escape" && event.key !== "F6") event.stopPropagation();
    });
    let cancelled = false;
    let dispose: (() => void) | undefined;
    editor = null;
    error = "";
    const failed = (reason: unknown) => {
      if (!cancelled) error = reason instanceof Error ? reason.message : String(reason);
    };
    void Promise.all([
      invoke<TldrawPreview>("open_tldraw", { path: target }),
      import("./tldraw-viewer.js"),
    ]).then(([preview, { mountTldraw }]) => {
      if (cancelled) return;
      dispose = mountTldraw(host, preview, (ready) => {
        if (cancelled) return;
        editor = ready;
        currentPageId = ready.getCurrentPageId();
      }, failed);
    }).catch(failed);
    return () => { cancelled = true; stopKeys(); dispose?.(); };
  });

  function selectPage(event: Event) {
    const id = (event.currentTarget as HTMLSelectElement).value as TLPageId;
    editor?.setCurrentPage(id);
    editor?.zoomToFit();
    currentPageId = id;
  }
</script>

<div
  role="application"
  class="flex h-full min-h-0 flex-col"
  aria-label={`tldraw preview of ${name}`}>
  <div class="flex items-center gap-2 border-b border-[var(--app-border)] px-2 py-1.5 text-xs">
    <select
      aria-label="Drawing page"
      class="min-w-0 flex-1 rounded bg-[var(--app-surface)] px-2 py-1 text-[var(--app-fg)]"
      value={currentPageId}
      disabled={!editor || !!error}
      onchange={selectPage}>
      {#each pages as page (page.id)}<option value={page.id}>{page.name}</option>{/each}
    </select>
    <span class="text-[var(--app-fg-muted)]">Read-only</span>
    <button class="rounded px-2 py-1 hover:bg-[var(--app-surface-raised)]" aria-label="Zoom out" disabled={!editor || !!error} onclick={() => editor?.zoomOut()}>−</button>
    <button class="rounded px-2 py-1 hover:bg-[var(--app-surface-raised)]" aria-label="Zoom in" disabled={!editor || !!error} onclick={() => editor?.zoomIn()}>+</button>
    <button class="rounded px-2 py-1 hover:bg-[var(--app-surface-raised)]" aria-label="Fit drawing to preview" disabled={!editor || !!error} onclick={() => editor?.zoomToFit()}>Fit</button>
  </div>
  <div class="relative min-h-0 flex-1">
    <div bind:this={container} class="absolute inset-0" class:invisible={!!error}></div>
    {#if error}
      <p role="alert" class="absolute inset-0 grid place-content-center px-4 text-center text-sm text-[var(--app-fg-muted)]">{error}</p>
    {:else if !editor}
      <p role="status" class="pointer-events-none absolute inset-0 grid place-content-center text-sm text-[var(--app-fg-muted)]">Loading drawing…</p>
    {/if}
  </div>
</div>
