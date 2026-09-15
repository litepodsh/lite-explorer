<script lang="ts">
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import XIcon from "@lucide/svelte/icons/x";
  import { open } from "@tauri-apps/plugin-dialog";
  import { DialogButton } from "$lib/components/custom/dialog/index.js";
  import { PathTree, type PathTreeDragHandlers } from "$lib/components/custom/path-tree/index.js";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import { drag } from "$lib/file-drag/drag.svelte.js";
  import { listArchive, type ArchiveListing } from "$lib/file-ops/archive.js";
  import { parentPath } from "$lib/file-ops/files.js";
  import { dropCandidateAt, resolveDestination } from "./drop-destination.js";
  import { collapseSelection, dragIcon, dragLabel, sizeByTreePath, summaryLine, toTreePaths } from "./extraction.js";
  import { extraction } from "./extraction.svelte.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  const SLOW_MS = 150;
  const SKELETON_INDENTS = [0, 1, 2, 2, 1, 0, 1, 0];

  let listing = $state.raw<ArchiveListing | null>(null);
  let error = $state("");
  let slow = $state(false);
  let selected = $state<string[]>([]);
  let tree = $state<ReturnType<typeof PathTree> | null>(null);

  $effect(() => {
    const current = path;
    let stale = false;
    listing = null;
    error = "";
    selected = [];
    slow = false;
    const timer = setTimeout(() => {
      if (!stale) slow = true;
    }, SLOW_MS);
    listArchive(current)
      .then((result) => {
        if (!stale) listing = result;
      })
      .catch((reason) => {
        if (!stale) error = reason instanceof Error ? reason.message : String(reason);
      })
      .finally(() => clearTimeout(timer));
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  });

  const treePaths = $derived(listing ? toTreePaths(listing.entries) : []);
  const sizes = $derived(listing ? sizeByTreePath(listing.entries) : new Map<string, number>());
  const hasSkipped = $derived(listing ? listing.entries.some((entry) => entry.skipped) : false);
  const chosen = $derived(collapseSelection(selected));
  const progress = $derived(extraction.running.get(path) ?? null);
  const fraction = $derived(
    !progress
      ? 0
      : progress.bytesTotal > 0
        ? progress.bytesDone / progress.bytesTotal
        : progress.filesTotal > 0
          ? progress.filesDone / progress.filesTotal
          : 0,
  );

  let dragged: string[] = [];

  const dragHandlers: PathTreeDragHandlers = {
    start: (paths, event) => {
      dragged = paths;
      updateDrag(event);
    },
    move: (event) => updateDrag(event),
    drop: (paths, event) => {
      const destination = resolveDestination(dropCandidateAt(event.clientX, event.clientY), drag.paneFolders);
      endDrag();
      if (destination) void extraction.run({ archive: path, destination, entries: collapseSelection(paths) });
    },
    cancel: () => endDrag(),
  };

  function updateDrag(event: PointerEvent) {
    const candidate = dropCandidateAt(event.clientX, event.clientY);
    const destination = resolveDestination(candidate, drag.paneFolders);
    const intoRow = destination !== null && candidate?.kind === "entry" && candidate.path === destination;
    drag.overEntryPath = intoRow ? destination : null;
    drag.overPaneId =
      destination !== null && !intoRow && candidate && candidate.kind !== "favorite" ? candidate.paneId : null;
    drag.ghost = {
      x: event.clientX,
      y: event.clientY,
      name: dragLabel(dragged),
      icon: dragIcon(dragged),
      action: destination ? "extract" : null,
    };
  }

  function endDrag() {
    drag.ghost = null;
    drag.overPaneId = null;
    drag.overEntryPath = null;
  }

  $effect(() => () => endDrag());

  function extract(destination: string) {
    void extraction.run({ archive: path, destination, entries: chosen.length ? chosen : undefined });
  }

  async function extractTo() {
    const destination = await open({
      directory: true,
      title: "Extract to…",
      defaultPath: parentPath(path) || undefined,
    });
    if (typeof destination === "string") extract(destination);
  }

  function decorate(treePath: string): string | null {
    const size = sizes.get(treePath);
    return size === undefined ? null : formatSize(size);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key.toLowerCase() !== "f" || !(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey) return;
    if (!tree) return;
    event.preventDefault();
    tree.openSearch();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="flex min-h-0 flex-1 flex-col">
  {#if error}
    <div class="grid flex-1 place-content-center justify-items-center gap-1 px-4 text-center">
      <ArchiveIcon class="mb-2 size-12 stroke-[1.2] text-[#67635f]" />
      <p class="text-[14px] font-semibold text-[#e8e5e2]">Can’t preview this archive</p>
      <p class="text-[12px] break-all text-[#9c9895]">{error}</p>
    </div>
  {:else if !listing}
    <div class="archive-skeleton flex flex-col gap-2.5 p-3" class:visible={slow} aria-hidden="true">
      {#each SKELETON_INDENTS as indent, index (index)}
        <div
          class="h-3.5 rounded-md bg-[#2d2a28]"
          style:margin-left="{indent * 14}px"
          style:width="{64 - indent * 10 + (index % 3) * 8}%"></div>
      {/each}
    </div>
  {:else if treePaths.length === 0}
    <div class="grid flex-1 place-content-center justify-items-center gap-1 px-4 text-center">
      <ArchiveIcon class="mb-2 size-12 stroke-[1.2] text-[#67635f]" />
      <p class="text-[13px] text-[#9c9895]">This archive is empty</p>
    </div>
  {:else}
    <header class="shrink-0 px-3 pt-3 pb-2">
      <p class="truncate text-[13px] font-semibold text-[#e8e5e2]" title={name}>{name}</p>
      <p
        class="truncate text-[12px] text-[#9c9895] tabular-nums"
        title={hasSkipped ? "Unsafe paths and links are not extracted" : undefined}>
        {summaryLine(listing)}
      </p>
    </header>
    {#if listing.truncated}
      <p class="shrink-0 border-y border-[#3a3734] px-3 py-1 text-[11px] text-[#9c9895]">Showing first 100,000 items</p>
    {/if}
    <div class="min-h-0 flex-1">
      <PathTree bind:this={tree} paths={treePaths} bind:selected {decorate} search initialExpansion={1} {dragHandlers} />
    </div>
    <footer class="flex h-11 shrink-0 items-center gap-2 border-t border-[#3a3734] px-3">
      {#if progress}
        <div class="archive-fade flex min-w-0 flex-1 items-center gap-3">
          <div class="h-1 flex-1 overflow-hidden rounded-full bg-[#3a3734]">
            <div class="archive-progress h-full w-full origin-left bg-[#0a9bff]" style:transform="scaleX({fraction})"></div>
          </div>
          <span class="text-[11px] text-[#9c9895] tabular-nums">{progress.filesDone} of {progress.filesTotal}</span>
          <DialogButton onclick={() => extraction.cancel(path)}>Cancel</DialogButton>
        </div>
      {:else}
        {#if chosen.length}
          <span class="archive-fade flex items-center gap-1 text-[12px] text-[#9c9895] tabular-nums">
            {chosen.length} selected
            <button
              class="grid size-5 place-items-center rounded border-0 bg-transparent text-[#9c9895] hover:bg-[#3b3836] hover:text-[#e8e5e2]"
              aria-label="Clear selection"
              onclick={() => (selected = [])}><XIcon class="size-3.5" /></button>
          </span>
        {/if}
        <div class="archive-fade ml-auto flex items-center gap-2">
          <DialogButton onclick={() => void extractTo()}>Extract to…</DialogButton>
          <DialogButton variant="primary" onclick={() => extract(parentPath(path) || path)}>Extract here</DialogButton>
        </div>
      {/if}
    </footer>
  {/if}
</div>

<style>
  .archive-skeleton {
    opacity: 0;
    transition: opacity 150ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .archive-skeleton.visible {
    opacity: 1;
  }
  .archive-fade {
    animation: archive-fade-in 120ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .archive-progress {
    transition: transform 120ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes archive-fade-in {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .archive-skeleton,
    .archive-progress {
      transition: none;
    }
    .archive-fade {
      animation: none;
    }
  }
</style>
