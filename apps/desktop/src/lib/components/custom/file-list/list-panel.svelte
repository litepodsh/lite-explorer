<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { isPartialDownload, type DownloadSnapshot } from "$lib/transfers/download-progress.js";
  import { tick, untrack } from "svelte";
  import { get } from "svelte/store";
  import ArrowDownIcon from "@lucide/svelte/icons/arrow-down";
  import ArrowUpIcon from "@lucide/svelte/icons/arrow-up";
  import ArrowUpDownIcon from "@lucide/svelte/icons/arrow-up-down";
  import { PreviewPanel } from "$lib/components/custom/preview/index.js";
  import * as Resizable from "$lib/components/ui/resizable/index.js";
  import { createRowVirtualizer } from "$lib/virtual/row-virtualizer.svelte.js";
  import ListItem, { type DirectoryEntry } from "./list-item.svelte";
  import { sortEntries, type SortColumn, type SortDir } from "./sort.js";
  import { loadFolderSort, saveFolderSort } from "./folder-sort.js";
  import { drag } from "$lib/file-drag/drag.svelte.js";
  import { dropTargetAt } from "$lib/file-drag/drop-target.js";
  import { trackPointerDrag } from "$lib/file-drag/pointer-drag.js";
  import { atWindowEdge, canDragOut, startNativeDrag, type DragIcon } from "$lib/file-drag/native-drag.js";
  import { canFavorite } from "$lib/favorites/favorites.js";
  import type { ListNavigator } from "$lib/file-pane/controller.svelte.js";
  import SelectionSummary from "$lib/components/custom/preview/selection-summary.svelte";
  import SelectionCheckbox from "./selection-checkbox.svelte";

  let {
    entries = [],
    view = "list",
    showHidden = false,
    selectedPaths = new Set<string>(),
    focusPath = "",
    checkboxes = false,
    renamingPath = "",
    paneId = "",
    onItemClick,
    onToggle,
    onRegisterNavigator,
    onToggleAll,
    onEnableCheckboxes,
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
    sortKey = "",
    pastedPaths = new Set<string>(),
    searchQuery = "",
    onFilesChanged,
  } = $props<{
    entries?: DirectoryEntry[];
    onFilesChanged?: () => void;
    view?: "list" | "grid";
    showHidden?: boolean;
    selectedPaths?: Set<string>;
    /** Entry keyboard navigation moves from. */
    focusPath?: string;
    /** Shows selection checkboxes on items and a select-all checkbox in the header. */
    checkboxes?: boolean;
    renamingPath?: string;
    paneId?: string;
    onItemClick?: (entry: DirectoryEntry, modifiers: { primary: boolean; shift: boolean }) => void;
    onToggle?: (entry: DirectoryEntry) => void;
    /** Hands the pane the visible order and layout, so keyboard selection matches what is shown. */
    onRegisterNavigator?: (navigator: ListNavigator) => () => void;
    onToggleAll?: () => void;
    /** The header checkbox was clicked while checkboxes are off. */
    onEnableCheckboxes?: () => void;
    onClearSelection?: () => void;
    onOpen?: (entry: DirectoryEntry, options?: { newTab: boolean }) => void;
    onReorder?: (entries: DirectoryEntry[]) => void;
    onRename?: (oldPath: string, newName: string) => void;
    onRenameCancel?: () => void;
    onContextMenu?: (entry: DirectoryEntry) => void;
    onExternalDrop?: (paths: string[], options: { move: boolean }) => void;
    previewPath?: string;
    previewOpen?: boolean;
    scrollTop?: number;
    onScroll?: (scrollTop: number) => void;
    /** Folder the sort is remembered for; empty to not remember it. */
    sortKey?: string;
    pastedPaths?: Set<string>;
    /** Content-search query to highlight inside result snippets. */
    searchQuery?: string;
  }>();

  let sortColumn = $state<SortColumn | null>(null);
  let sortDir = $state<SortDir>("asc");

  // Each folder opens with the sort last chosen in it.
  $effect.pre(() => {
    const saved = loadFolderSort(sortKey);
    sortColumn = saved?.column ?? "name";
    sortDir = saved?.dir ?? "asc";
  });

  function toggleSort(column: SortColumn) {
    if (sortColumn === column) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortColumn = column;
      sortDir = "asc";
    }
    saveFolderSort(sortKey, { column, dir: sortDir });
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
    `grid-template-columns: ${checkboxes ? "3.5rem" : "2rem"} ${colWidths.name == null ? "minmax(0,1fr)" : `${colWidths.name}px`} ${colWidths.type}px ${colWidths.size}px ${colWidths.date}px`,
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
  let selectedVisible = $derived(visibleEntries.filter((entry: DirectoryEntry) => selectedPaths.has(entry.path)));
  let allSelected = $derived(visibleEntries.length > 0 && selectedVisible.length === visibleEntries.length);
  let externalOver = $derived(drag.overPaneId === paneId);

  // Other lists drop entries on this one through the shared registry.
  $effect(() => {
    const id = paneId;
    const handler = (paths: string[], options: { move: boolean }) => onExternalDrop?.(paths, options);
    drag.paneDrops.set(id, handler);
    return () => {
      if (drag.paneDrops.get(id) === handler) drag.paneDrops.delete(id);
    };
  });

  let cancelEntryDrag = () => {};
  $effect(() => () => cancelEntryDrag());

  function toDragged(entry: DirectoryEntry) {
    return { path: entry.path, name: entry.name, paneId, isDirectory: entry.is_directory, kind: entry.kind };
  }

  function startEntryDrag(event: PointerEvent, entry: DirectoryEntry) {
    if ((event.target as HTMLElement).closest("input") || entry.path === renamingPath) return;
    cancelEntryDrag = trackPointerDrag(event, {
      onStart: () => {
        // Dragging a selected entry carries the whole selection; any other entry is selected first.
        const group = selectedPaths.has(entry.path) ? selectedVisible : [entry];
        if (!selectedPaths.has(entry.path)) onItemClick?.(entry, { primary: false, shift: false });
        drag.entry = toDragged(entry);
        drag.entries = group.map(toDragged);
      },
      onMove: (move) => updateEntryDrag(move, entry),
      onDrop: (up) => dropEntry(up, entry),
      onCancel: endEntryDrag,
    });
  }

  function entryIcon(entry: DirectoryEntry): DragIcon {
    return entry.kind === "share" ? "drive" : entry.is_directory ? "folder" : "file";
  }

  function draggedPaths(entry: DirectoryEntry): string[] {
    return drag.entries.length ? drag.entries.map((dragged) => dragged.path) : [entry.path];
  }

  function updateEntryDrag(event: PointerEvent, entry: DirectoryEntry) {
    const group = drag.entries.length ? drag.entries : [toDragged(entry)];
    // At the window edge the drag becomes a native one, so the items can be dropped in other apps.
    if (
      group.every((dragged) => canDragOut(dragged)) &&
      atWindowEdge(event.clientX, event.clientY, window.innerWidth, window.innerHeight)
    ) {
      cancelEntryDrag();
      startNativeDrag(group.map((dragged) => dragged.path), entry.name, entryIcon(entry));
      return;
    }
    const target = dropTargetAt(event.clientX, event.clientY);
    const toFavorites = target?.kind === "favorites" && group.length === 1 && canFavorite(entry);
    const toPane = target !== null && target.kind !== "favorites" && target.paneId !== paneId;
    drag.favoriteDropAt = toFavorites ? target.index : null;
    drag.overPaneId = toPane ? target.paneId : null;
    drag.overEntryPath =
      group.length === 1 && target?.kind === "entry" && target.paneId === paneId && target.path !== entry.path
        ? target.path
        : null;
    drag.ghost = {
      x: event.clientX,
      y: event.clientY,
      name: entry.name,
      icon: entryIcon(entry),
      action: toFavorites ? "favorite" : toPane ? (event.metaKey || event.ctrlKey ? "move" : "copy") : null,
      count: group.length,
    };
  }

  function dropEntry(event: PointerEvent, entry: DirectoryEntry) {
    const target = dropTargetAt(event.clientX, event.clientY);
    const paths = draggedPaths(entry);
    endEntryDrag();
    if (!target) return;
    if (target.kind === "favorites") {
      if (paths.length === 1 && canFavorite(entry)) drag.favoritesDrop?.(entry.path, target.index);
    } else if (target.paneId !== paneId) {
      drag.paneDrops.get(target.paneId)?.(paths, { move: event.metaKey || event.ctrlKey });
    } else if (target.kind === "entry" && paths.length === 1) {
      reorder(entry.path, target.path);
    }
  }

  function endEntryDrag() {
    drag.entry = null;
    drag.entries = [];
    drag.ghost = null;
    drag.overPaneId = null;
    drag.overEntryPath = null;
    drag.favoriteDropAt = null;
  }

  // The preview pane slides in when a preview is requested and out when it clears. `previewEntry`
  // lags the request so the pane keeps its content while the close animation runs. Preview is
  // driven by `previewPath` (not the selection) so a right-click can highlight without previewing.
  const PREVIEW_ANIMATION_MS = 160;
  const PREVIEW_EASING = "cubic-bezier(0.2, 0, 0, 1)";
  const SUMMARY = "summary";
  let previewEntry = $state<DirectoryEntry | typeof SUMMARY | null>(null);
  let listPaneEl = $state<HTMLElement | null>(null);
  let previewPaneEl = $state<HTMLElement | null>(null);
  let closeAnimations: Animation[] = [];

  let previewRequestedEntry = $derived(
    previewPath ? (visibleEntries.find((entry: DirectoryEntry) => entry.path === previewPath) ?? null) : null,
  );
  // Several selected entries show a summary in the preview pane instead of one file.
  let summarizing = $derived(selectedVisible.length > 1);
  let previewTarget = $derived(summarizing ? SUMMARY : previewRequestedEntry);

  $effect(() => {
    const target = previewOpen ? previewTarget : null;
    untrack(() => (target ? void showPreview(target) : hidePreview()));
  });

  function previewDuration(): number {
    return matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : PREVIEW_ANIMATION_MS;
  }

  async function showPreview(entry: DirectoryEntry | typeof SUMMARY) {
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
  const SEARCH_ROW_H = 50; // name plus a path/snippet line
  const GRID_ITEM_H = 96; // h-24
  const GRID_GAP = 4; // gap-1
  const GRID_ROW_H = GRID_ITEM_H + GRID_GAP;
  const GRID_TOP = 8; // mt-2 above the grid rows
  const GRID_MIN = 110;
  const SCROLL_PADDING_X = 16; // px-2

  let scrollEl = $state<HTMLDivElement>();
  let containerW = $state(0);
  // Rows scroll under the toolbar and the column header; this is the scroll area's top padding they cover.
  let insetTop = $state(0);

  $effect(() => {
    const element = scrollEl;
    if (!element) return;
    const measure = () => (insetTop = parseFloat(getComputedStyle(element).paddingTop) || 0);
    // A padding change (view switch, tab bar toggling the overlap) resizes the content box.
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  });

  let itemsPerRow = $derived(view === "grid" ? Math.max(1, Math.floor((containerW - SCROLL_PADDING_X + GRID_GAP) / (GRID_MIN + GRID_GAP))) : 1);
  let gridRowCount = $derived(Math.ceil(visibleEntries.length / itemsPerRow));
  let scrollMargin = $derived(view === "list" ? insetTop : insetTop + GRID_TOP);

  const rows = createRowVirtualizer({
    count: () => (view === "list" ? visibleEntries.length : gridRowCount),
    estimateSize: () => (view === "list" ? (visibleEntries[0]?.relative_path != null ? SEARCH_ROW_H : LIST_ROW_H) : GRID_ROW_H),
    scrollMargin: () => scrollMargin,
    scrollPaddingStart: () => insetTop,
    overscan: 10,
    getScrollElement: () => scrollEl ?? null,
  });

  $effect(() => {
    const register = onRegisterNavigator;
    if (!register) return;
    return register({
      order: () => visibleEntries,
      view: () => view,
      columns: () => itemsPerRow,
      pageRows: () => {
        const rowHeight = view === "list" ? (visibleEntries[0]?.relative_path != null ? SEARCH_ROW_H : LIST_ROW_H) : GRID_ROW_H;
        const viewport = (scrollEl?.clientHeight ?? 0) - insetTop;
        return Math.max(1, Math.floor(viewport / rowHeight));
      },
      scrollToIndex: (index: number) => {
        const row = view === "grid" ? Math.floor(index / itemsPerRow) : index;
        if (rows.virtualizer) get(rows.virtualizer).scrollToIndex(row, { align: "auto" });
      },
      sort: (column: SortColumn, dir: SortDir) => {
        sortColumn = column;
        sortDir = dir;
        saveFolderSort(sortKey, { column, dir });
      },
    });
  });

  let downloadSnapshots = $state<Record<string, DownloadSnapshot>>({});
  // Only inspect rendered local items. No directory scans or browser history access.
  const downloadPaths = $derived(rows.virtualItems.flatMap((row) =>
    view === "grid" ? gridRowEntries(row.index) : [visibleEntries[row.index]],
  ).filter((entry): entry is DirectoryEntry => !!entry && !entry.inner_path && canDragOut(entry) &&
    isPartialDownload(entry.name))
    .map((entry) => entry.path).slice(0, 512).join("\0"));

  $effect(() => {
    const paths = downloadPaths ? downloadPaths.split("\0") : [];
    const refresh = onFilesChanged;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    downloadSnapshots = {};
    async function poll() {
      try {
        if (!document.hidden) {
          const snapshots = await invoke<DownloadSnapshot[]>("read_download_progress", { paths });
          if (stopped) return;
          downloadSnapshots = Object.fromEntries(snapshots.map((snapshot) => [snapshot.path, snapshot]));
          if (snapshots.some((snapshot) => snapshot.missing)) refresh?.();
        }
      } catch (error) {
        // Optional metadata must never prevent browsing a folder.
        if (!stopped) console.warn("Could not read download progress", error);
      } finally {
        if (!stopped) timer = setTimeout(poll, 1000);
      }
    }
    if (paths.length) void poll();
    return () => { stopped = true; clearTimeout(timer); };
  });

  // Checkboxes cascade in and out from the top of the viewport.
  function revealDelay(index: number): number {
    const first = rows.virtualItems[0]?.index ?? 0;
    return Math.min(Math.max(0, index - first), 12) * 14;
  }

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

  function reorder(fromPath: string, toPath: string) {
    const from = entries.findIndex((entry: DirectoryEntry) => entry.path === fromPath);
    const to = entries.findIndex((entry: DirectoryEntry) => entry.path === toPath);
    if (from < 0 || to < 0 || from === to) return;
    const next = [...entries];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    onReorder?.(next);
  }
</script>

<Resizable.PaneGroup direction="horizontal" autoSaveId="preview-panel" class="min-h-0 min-w-0 flex-1">
  <Resizable.Pane id="file-list" order={1} minSize={30} bind:ref={listPaneEl}>
    <div
      role={view === "list" ? "grid" : undefined}
      aria-multiselectable={view === "list" ? "true" : undefined}
      aria-rowcount={view === "list" ? visibleEntries.length + 1 : undefined}
      class="relative flex h-full min-h-0 min-w-0 flex-col text-left">
      {#snippet sortIcon(column: SortColumn)}
        {#if sortColumn === column}
          {#if sortDir === "asc"}<ArrowUpIcon class="size-3" />{:else}<ArrowDownIcon class="size-3" />{/if}
        {:else}
          <ArrowUpDownIcon class="size-3 opacity-40" />
        {/if}
      {/snippet}
      {#if view === "list"}
        <!-- Glass header floats over the scroll area, so rows stay visible, blurred, behind it. -->
        <div
          role="row"
          aria-rowindex={1}
          style={gridTemplate}
          class="list-header absolute inset-x-0 z-10 grid h-8 items-center text-[11px] font-semibold uppercase tracking-wide text-[#9c9895]">
          <div role="columnheader" class="flex items-center px-2">
            <!-- First click turns on checkbox selection; after that it selects or deselects everything. -->
            <SelectionCheckbox
              checked={checkboxes && allSelected}
              indeterminate={checkboxes && !allSelected && selectedVisible.length > 0}
              label={!checkboxes ? "Turn on multiple selection" : allSelected ? "Deselect all" : "Select all"}
              onToggle={() => (checkboxes ? onToggleAll?.() : onEnableCheckboxes?.())} />
          </div>
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
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
      <div
        bind:this={scrollEl}
        bind:clientWidth={containerW}
        data-file-list
        data-pane-id={paneId}
        data-key-scope="list"
        tabindex="0"
        onclick={handleBlankClick}
        onscroll={() => scrollEl && onScroll?.(scrollEl.scrollTop)}
        class="list-scroll min-h-0 min-w-0 flex-1 overflow-auto px-2 pb-2 [scrollbar-gutter:stable]"
        class:with-header={view === "list"}
        class:remote-drop-active={externalOver}>
        {#if view === "list"}
          {#if visibleEntries.length === 0}
            <p class="p-4 text-center text-[13px] text-[#9c9895]">This folder is empty.</p>
          {:else}
            <div role="rowgroup" class="relative" style="height: {rows.totalSize}px;">
              {#each rows.virtualItems as v (v.key)}
                {@const entry = visibleEntries[v.index]}
                {#if entry}
                  {@const selected = selectedPaths.has(entry.path)}
                  <ListItem
                    {entry}
                    downloadSnapshot={downloadSnapshots[entry.path]}
                    view="list"
                    rowIndex={v.index + 2}
                    zebra={v.index % 2 === 1}
                    {selected}
                    focused={entry.path === focusPath}
                    joinPrev={selected && v.index > 0 && selectedPaths.has(visibleEntries[v.index - 1].path)}
                    joinNext={selected && v.index + 1 < visibleEntries.length && selectedPaths.has(visibleEntries[v.index + 1].path)}
                    {checkboxes}
                    revealDelay={revealDelay(v.index)}
                    renaming={entry.path === renamingPath}
                    style="position: absolute; top: 0; left: 0; width: max-content; min-width: 100%; height: {v.size}px; transform: translateY({v.start - scrollMargin}px); {gridTemplate}"
                    {onItemClick}
                    {onToggle}
                    {onOpen}
                    {onRename}
                    {onRenameCancel}
                    {onContextMenu}
                    dropTarget={entry.path === drag.overEntryPath}
                    pasted={pastedPaths.has(entry.path)}
                    {searchQuery}
                    onPointerDown={startEntryDrag} />
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
                    downloadSnapshot={downloadSnapshots[entry.path]}
                    view="grid"
                    selected={selectedPaths.has(entry.path)}
                    focused={entry.path === focusPath}
                    {checkboxes}
                    revealDelay={revealDelay(v.index)}
                    renaming={entry.path === renamingPath}
                    {onItemClick}
                    {onToggle}
                    {onOpen}
                    {onRename}
                    {onRenameCancel}
                    {onContextMenu}
                    dropTarget={entry.path === drag.overEntryPath}
                    pasted={pastedPaths.has(entry.path)}
                    {searchQuery}
                    onPointerDown={startEntryDrag} />
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
    <Resizable.Pane id="preview" order={2} defaultSize={35} minSize={20} maxSize={70} class="pt-(--content-overlap)" bind:ref={previewPaneEl}>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="h-full min-h-0" data-key-scope="preview" data-pane-id={paneId} tabindex="0">
        {#if previewEntry === SUMMARY}
          <SelectionSummary entries={selectedVisible} />
        {:else}
          <PreviewPanel entry={previewEntry} />
        {/if}
      </div>
    </Resizable.Pane>
  {/if}
</Resizable.PaneGroup>

<style>
  /* `--content-overlap` is the toolbar height the pane slides under (0 when the tab bar is open). */
  .list-scroll {
    padding-top: var(--content-overlap, 0px);
    scroll-padding-top: var(--content-overlap, 0px);
  }
  .list-scroll.with-header {
    padding-top: calc(var(--content-overlap, 0px) + 2rem);
    scroll-padding-top: calc(var(--content-overlap, 0px) + 2rem);
  }

  .list-header {
    top: var(--content-overlap, 0px);
    isolation: isolate;
    box-shadow: inset 0 -1px 0 rgb(255 255 255 / 7%);
    transition: grid-template-columns 260ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  /* One frosted band from the top of the pane (under the toolbar) to the header's bottom edge. Full blur
     from that edge up, so the header stands apart from the rows passing behind it. */
  .list-header::before {
    position: absolute;
    inset: calc(-1 * var(--content-overlap, 0px)) 0 0;
    z-index: -1;
    background: linear-gradient(to top, rgb(36 34 32 / 62%), rgb(37 35 34 / 42%));
    -webkit-backdrop-filter: blur(24px) saturate(140%);
    backdrop-filter: blur(24px) saturate(140%);
    content: "";
    pointer-events: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .list-header {
      transition: none;
    }
  }

  @media (prefers-reduced-transparency: reduce) {
    .list-header::before {
      background: #242220;
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
    }
  }
</style>
