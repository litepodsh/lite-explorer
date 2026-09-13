<script lang="ts">
  import { tick, untrack } from "svelte";
  import ArrowDownIcon from "@lucide/svelte/icons/arrow-down";
  import ArrowUpIcon from "@lucide/svelte/icons/arrow-up";
  import ArrowUpDownIcon from "@lucide/svelte/icons/arrow-up-down";
  import { PreviewPanel } from "$lib/components/custom/preview/index.js";
  import * as Resizable from "$lib/components/ui/resizable/index.js";
  import { createRowVirtualizer } from "$lib/virtual/row-virtualizer.svelte.js";
  import ListItem, { type DirectoryEntry } from "./list-item.svelte";
  import { sortEntries, type SortColumn, type SortDir } from "./sort.js";
  import { drag } from "$lib/file-drag/drag.svelte.js";

  let {
    entries = [],
    view = "list",
    showHidden = false,
    selectedPath = "",
    renamingPath = "",
    paneId = "",
    onSelect,
    onClearSelection,
    onOpen,
    onReorder,
    onRename,
    onRenameCancel,
    onContextMenu,
    onExternalDrop,
    previewPath = "",
    previewOpen = true,
    scrollTop = 0,
    onScroll,
  } = $props<{
    entries?: DirectoryEntry[];
    view?: "list" | "grid";
    showHidden?: boolean;
    selectedPath?: string;
    renamingPath?: string;
    paneId?: string;
    onSelect?: (entry: DirectoryEntry) => void;
    onClearSelection?: () => void;
    onOpen?: (entry: DirectoryEntry, options?: { newTab: boolean }) => void;
    onReorder?: (entries: DirectoryEntry[]) => void;
    onRename?: (oldPath: string, newName: string) => void;
    onRenameCancel?: () => void;
    onContextMenu?: (entry: DirectoryEntry) => void;
    onExternalDrop?: (path: string, options: { move: boolean }) => void;
    previewPath?: string;
    previewOpen?: boolean;
    scrollTop?: number;
    onScroll?: (scrollTop: number) => void;
  }>();

  let sortColumn = $state<SortColumn | null>(null);
  let sortDir = $state<SortDir>("asc");

  function toggleSort(column: SortColumn) {
    if (sortColumn === column) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortColumn = column;
      sortDir = "asc";
    }
  }

  type ColWidths = { name: number | null; type: number; size: number; date: number };
  const COL_WIDTHS_STORAGE = "file-list-col-widths";
  const DEFAULT_COL_WIDTHS: ColWidths = { name: null, type: 96, size: 96, date: 168 };

  function loadColWidths(): ColWidths {
    try {
      const raw = localStorage.getItem(COL_WIDTHS_STORAGE);
      if (raw) return { ...DEFAULT_COL_WIDTHS, ...JSON.parse(raw) } as ColWidths;
    } catch {
      /* ignore malformed storage */
    }
    return { ...DEFAULT_COL_WIDTHS };
  }

  let colWidths = $state<ColWidths>(loadColWidths());

  let gridTemplate = $derived(
    `grid-template-columns: 2rem ${colWidths.name == null ? "minmax(0,1fr)" : `${colWidths.name}px`} ${colWidths.type}px ${colWidths.size}px ${colWidths.date}px`,
  );

  function startResize(col: keyof ColWidths, event: PointerEvent) {
    const header = (event.currentTarget as HTMLElement).parentElement;
    if (!header) return;
    const startX = event.clientX;
    const startWidth = header.getBoundingClientRect().width;
    const min = col === "name" ? 80 : 48;
    function onMove(move: PointerEvent) {
      colWidths[col] = Math.max(min, Math.round(startWidth + (move.clientX - startX)));
    }
    function onUp() {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      localStorage.setItem(COL_WIDTHS_STORAGE, JSON.stringify(colWidths));
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  let filteredEntries = $derived(showHidden ? entries : entries.filter((entry: DirectoryEntry) => !entry.is_hidden));
  let visibleEntries = $derived(sortColumn ? sortEntries(filteredEntries, sortColumn, sortDir) : filteredEntries);
  let selectedEntry = $derived(visibleEntries.find((entry: DirectoryEntry) => entry.path === selectedPath) ?? null);
  let draggedPath = $state("");
  let externalOver = $state(false);

  function startDrag(entry: DirectoryEntry) {
    draggedPath = entry.path;
    drag.entry = { path: entry.path, paneId };
  }

  function handleExternalDragOver(event: DragEvent) {
    if (!drag.entry || drag.entry.paneId === paneId) return;
    event.preventDefault();
    externalOver = true;
  }

  function handleExternalDragLeave() {
    externalOver = false;
  }

  function handleExternalDrop(event: DragEvent) {
    if (!drag.entry || drag.entry.paneId === paneId) return;
    event.preventDefault();
    externalOver = false;
    onExternalDrop?.(drag.entry.path, { move: event.metaKey || event.ctrlKey });
    drag.entry = null;
  }

  function handleExternalDragEnd() {
    externalOver = false;
    drag.entry = null;
  }

  // The preview pane slides in when a preview is requested and out when it clears. `previewEntry`
  // lags the request so the pane keeps its content while the close animation runs. Preview is
  // driven by `previewPath` (not `selectedPath`) so a right-click can highlight without previewing.
  const PREVIEW_ANIMATION_MS = 160;
  const PREVIEW_EASING = "cubic-bezier(0.2, 0, 0, 1)";
  let previewEntry = $state<DirectoryEntry | null>(null);
  let listPaneEl = $state<HTMLElement | null>(null);
  let previewPaneEl = $state<HTMLElement | null>(null);
  let closeAnimations: Animation[] = [];

  let previewRequestedEntry = $derived(
    previewPath ? (visibleEntries.find((entry: DirectoryEntry) => entry.path === previewPath) ?? null) : null,
  );

  $effect(() => {
    const entry = previewOpen ? previewRequestedEntry : null;
    untrack(() => (entry ? void showPreview(entry) : hidePreview()));
  });

  function previewDuration(): number {
    return matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : PREVIEW_ANIMATION_MS;
  }

  async function showPreview(entry: DirectoryEntry) {
    const wasHidden = previewEntry === null;
    previewEntry = entry;
    if (closeAnimations.length > 0) {
      // Reopened mid-close: grow back from wherever the close animation reached.
      const preview = previewPaneEl ? getComputedStyle(previewPaneEl) : null;
      const from = { flexGrow: preview?.flexGrow ?? "0", opacity: preview?.opacity ?? "0" };
      const listFrom = listPaneEl ? getComputedStyle(listPaneEl).flexGrow : "100";
      for (const animation of closeAnimations) animation.cancel();
      closeAnimations = [];
      growPreview(from, listFrom);
    } else if (wasHidden) {
      await tick();
      growPreview({ flexGrow: "0", opacity: "0" }, "100");
    }
  }

  // Single start keyframes animate toward the sizes paneforge applies, including saved layouts.
  function growPreview(from: { flexGrow: string; opacity: string }, listFrom: string) {
    const options = { duration: previewDuration(), easing: PREVIEW_EASING };
    previewPaneEl?.animate([{ ...from, offset: 0 }], options);
    listPaneEl?.animate([{ flexGrow: listFrom, offset: 0 }], options);
  }

  function hidePreview() {
    if (!previewEntry || closeAnimations.length > 0) return;
    if (!previewPaneEl || !listPaneEl) {
      previewEntry = null;
      return;
    }
    const options = { duration: previewDuration(), easing: PREVIEW_EASING, fill: "forwards" as const };
    const animations = [
      previewPaneEl.animate([{ flexGrow: "0", opacity: "0" }], options),
      listPaneEl.animate([{ flexGrow: "100" }], options),
    ];
    closeAnimations = animations;
    animations[0].onfinish = async () => {
      if (closeAnimations !== animations) return;
      closeAnimations = [];
      previewEntry = null;
      previewPaneEl = null;
      // The list pane holds its end state until the preview pane is gone, then fills on its own.
      await tick();
      for (const animation of animations) animation.cancel();
    };
  }

  const LIST_ROW_H = 36; // h-9
  const GRID_ITEM_H = 96; // h-24
  const GRID_GAP = 4; // gap-1
  const GRID_ROW_H = GRID_ITEM_H + GRID_GAP;
  const GRID_TOP = 8; // mt-2 above the grid rows
  const GRID_MIN = 110;
  const SCROLL_PADDING_X = 16; // px-2

  let scrollEl = $state<HTMLDivElement>();
  let containerW = $state(0);

  let itemsPerRow = $derived(view === "grid" ? Math.max(1, Math.floor((containerW - SCROLL_PADDING_X + GRID_GAP) / (GRID_MIN + GRID_GAP))) : 1);
  let gridRowCount = $derived(Math.ceil(visibleEntries.length / itemsPerRow));
  let scrollMargin = $derived(view === "list" ? 32 : GRID_TOP);

  const rows = createRowVirtualizer({
    count: () => (view === "list" ? visibleEntries.length : gridRowCount),
    estimateSize: () => (view === "list" ? LIST_ROW_H : GRID_ROW_H),
    scrollMargin: () => scrollMargin,
    overscan: 10,
    getScrollElement: () => scrollEl ?? null,
  });

  // The page unmounts this panel while a directory loads, so restoring once per mount
  // restores the active tab's position after each tab switch.
  let scrollRestored = false;
  $effect(() => {
    const element = scrollEl;
    if (!element || scrollRestored || visibleEntries.length === 0) return;
    scrollRestored = true;
    const top = untrack(() => scrollTop);
    if (top > 0) requestAnimationFrame(() => (element.scrollTop = top));
  });

  // Clicking blank space (not a row or grid item) deselects, like Finder.
  function handleBlankClick(event: MouseEvent) {
    if (!(event.target instanceof Element) || event.target.closest("[role='row'], button")) return;
    if (document.activeElement instanceof HTMLElement && scrollEl?.contains(document.activeElement)) {
      document.activeElement.blur();
    }
    onClearSelection?.();
  }

  function gridRowEntries(rowIndex: number): DirectoryEntry[] {
    const start = rowIndex * itemsPerRow;
    return visibleEntries.slice(start, start + itemsPerRow);
  }

  function reorder(target: DirectoryEntry) {
    const from = entries.findIndex((entry: DirectoryEntry) => entry.path === draggedPath);
    const to = entries.findIndex((entry: DirectoryEntry) => entry.path === target.path);
    if (from < 0 || to < 0 || from === to) return;
    const next = [...entries];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    onReorder?.(next);
    draggedPath = "";
  }
</script>

<Resizable.PaneGroup direction="horizontal" autoSaveId="preview-panel" class="min-h-0 min-w-0 flex-1">
  <Resizable.Pane id="file-list" order={1} minSize={30} bind:ref={listPaneEl}>
    <div
      role={view === "list" ? "grid" : undefined}
      aria-rowcount={view === "list" ? visibleEntries.length + 1 : undefined}
      class="flex h-full min-h-0 min-w-0 flex-col text-left">
      {#snippet sortIcon(column: SortColumn)}
        {#if sortColumn === column}
          {#if sortDir === "asc"}<ArrowUpIcon class="size-3" />{:else}<ArrowDownIcon class="size-3" />{/if}
        {:else}
          <ArrowUpDownIcon class="size-3 opacity-40" />
        {/if}
      {/snippet}
      {#if view === "list"}
        <!-- Sticky header lives inside the scroll container so it stays aligned with the
             rows during both vertical and horizontal scrolling. -->
        <div
          role="row"
          aria-rowindex={1}
          style={gridTemplate}
          class="sticky top-0 z-10 grid h-8 items-center bg-[#242220] text-[11px] font-semibold uppercase tracking-wide text-[#9c9895] shadow-[inset_0_-1px_0_#3a3734]">
          <div role="columnheader" class="px-2"></div>
          <div role="columnheader" class="relative px-2">
            <button type="button" class="flex items-center gap-1 uppercase hover:text-[#e8e5e2]" onclick={() => toggleSort("name")} aria-label="Sort by name">
              Name {@render sortIcon("name")}
            </button>
            <div role="separator" aria-label="Resize name column" class="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-blue-400/40" onpointerdown={(event) => startResize("name", event)}></div>
          </div>
          <div role="columnheader" class="relative px-2">
            <button type="button" class="flex items-center gap-1 uppercase hover:text-[#e8e5e2]" onclick={() => toggleSort("type")} aria-label="Sort by type">
              Type {@render sortIcon("type")}
            </button>
            <div role="separator" aria-label="Resize type column" class="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-blue-400/40" onpointerdown={(event) => startResize("type", event)}></div>
          </div>
          <div role="columnheader" class="relative px-2">
            <button type="button" class="ml-auto flex items-center gap-1 uppercase hover:text-[#e8e5e2]" onclick={() => toggleSort("size")} aria-label="Sort by size">
              Size {@render sortIcon("size")}
            </button>
            <div role="separator" aria-label="Resize size column" class="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-blue-400/40" onpointerdown={(event) => startResize("size", event)}></div>
          </div>
          <div role="columnheader" class="relative px-2">
            <button type="button" class="ml-auto flex items-center gap-1 uppercase hover:text-[#e8e5e2]" onclick={() => toggleSort("date")} aria-label="Sort by date added">
              Date Added {@render sortIcon("date")}
            </button>
            <div role="separator" aria-label="Resize date added column" class="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-blue-400/40" onpointerdown={(event) => startResize("date", event)}></div>
          </div>
        </div>
      {/if}
      <!-- Blank-space click to deselect is a mouse-only convenience, like Finder. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        bind:this={scrollEl}
        bind:clientWidth={containerW}
        data-file-list
        onclick={handleBlankClick}
        onscroll={() => scrollEl && onScroll?.(scrollEl.scrollTop)}
        ondragover={handleExternalDragOver}
        ondragleave={handleExternalDragLeave}
        ondrop={handleExternalDrop}
        ondragend={handleExternalDragEnd}
        class="min-h-0 min-w-0 flex-1 overflow-auto px-2 pb-2 [scrollbar-gutter:stable]"
        class:remote-drop-active={externalOver}>
        {#if view === "list"}
          {#if visibleEntries.length === 0}
            <p class="p-4 text-center text-[13px] text-[#9c9895]">This folder is empty.</p>
          {:else}
            <div role="rowgroup" class="relative" style="height: {rows.totalSize}px;">
              {#each rows.virtualItems as v (v.key)}
                {@const entry = visibleEntries[v.index]}
                {#if entry}
                  <ListItem
                    {entry}
                    view="list"
                    rowIndex={v.index + 2}
                    selected={entry.path === selectedPath}
                    renaming={entry.path === renamingPath}
                    style="position: absolute; top: 0; left: 0; width: max-content; min-width: 100%; height: {v.size}px; transform: translateY({v.start - scrollMargin}px); {gridTemplate}"
                    {onSelect}
                    {onOpen}
                    {onRename}
                    {onRenameCancel}
                    {onContextMenu}
                    onDragStart={startDrag}
                    onDrop={reorder} />
                {/if}
              {/each}
            </div>
          {/if}
        {:else if visibleEntries.length === 0}
          <p class="mt-2 p-4 text-center text-[13px] text-[#9c9895]">This folder is empty.</p>
        {:else}
          <div class="relative mt-2" style="height: {rows.totalSize}px;">
            {#each rows.virtualItems as v (v.key)}
              <div
                class="grid gap-1"
                style="position: absolute; top: 0; left: 0; right: 0; height: {v.size}px; grid-template-columns: repeat({itemsPerRow}, minmax(0, 1fr)); align-content: start; transform: translateY({v.start - scrollMargin}px);">
                {#each gridRowEntries(v.index) as entry (entry.path)}
                  <ListItem
                    {entry}
                    view="grid"
                    selected={entry.path === selectedPath}
                    renaming={entry.path === renamingPath}
                    {onSelect}
                    {onOpen}
                    {onRename}
                    {onRenameCancel}
                    {onContextMenu}
                    onDragStart={startDrag}
                    onDrop={reorder} />
                {/each}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </Resizable.Pane>
  {#if previewEntry}
    <Resizable.Handle />
    <Resizable.Pane id="preview" order={2} defaultSize={35} minSize={20} maxSize={70} bind:ref={previewPaneEl}>
      <PreviewPanel entry={previewEntry} />
    </Resizable.Pane>
  {/if}
</Resizable.PaneGroup>
