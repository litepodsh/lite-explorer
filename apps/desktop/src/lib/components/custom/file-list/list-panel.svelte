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
  import { sortEntries, entryType, type SortColumn, type SortDir } from "./sort.js";
  import { DEFAULT_COLUMNS, COLUMN_LABELS, loadColumns, saveColumns, moveColumn, type ListColumn } from "./columns.js";
  import { loadFolderSort, saveFolderSort } from "./folder-sort.js";
  import { drag, type DraggedEntry } from "$lib/file-drag/drag.svelte.js";
  import { dropTargetAt } from "$lib/file-drag/drop-target.js";
  import { trackPointerDrag } from "$lib/file-drag/pointer-drag.js";
  import { atWindowEdge, canDragOut, startNativeDrag, type DragIcon } from "$lib/file-drag/native-drag.js";
  import { canFavorite } from "$lib/favorites/favorites.js";
  import type { ListNavigator } from "$lib/file-pane/controller.svelte.js";
  import { formatDate, formatSize } from "$lib/components/custom/preview/format.js";
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
    onNeedDetails,
    previewing = false,
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
    onExternalDrop?: (entries: DraggedEntry[], destination?: string) => void;
    previewPath?: string;
    previewOpen?: boolean;
    scrollTop?: number;
    onScroll?: (scrollTop: number) => void;
    /** Folder the sort is remembered for; empty to not remember it. */
    sortKey?: string;
    pastedPaths?: Set<string>;
    /** Content-search query to highlight inside result snippets. */
    searchQuery?: string;
    /** Rendered as a non-interactive preview of another folder (swipe animation). */
    previewing?: boolean;
    /** Paths of shown rows that carry no size or dates yet, for a streamed network listing. */
    onNeedDetails?: (paths: string[]) => void;
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

  let columns = $state<ListColumn[]>([...DEFAULT_COLUMNS]);
  let columnTarget = $state<ListColumn | null>(null);
  let columnGhost = $state<{ column: ListColumn; x: number; y: number; width: number } | null>(null);
  function columnPreview(entry: DirectoryEntry, column: ListColumn) {
    if (column === "name") return entry.name;
    if (column === "type") return entryType(entry);
    if (column === "size") return entry.size == null ? "—" : formatSize(entry.size);
    const value = column === "date" ? entry.created : entry.modified;
    return value == null ? "—" : formatDate(value);
  }
  let cancelColumnDrag = () => {};
  $effect.pre(() => { columns = loadColumns(sortKey); });
  $effect(() => () => cancelColumnDrag());
  function reorderColumn(from: ListColumn, to: ListColumn) {
    columns = moveColumn(columns, from, to);
    saveColumns(sortKey, columns);
  }
  function startColumnDrag(event: PointerEvent, column: ListColumn) {
    const header = (event.currentTarget as HTMLElement).closest<HTMLElement>("[data-list-column]");
    const row = header?.closest('[role="row"]');
    const width = Math.min(260, Math.max(110, header?.getBoundingClientRect().width ?? 160));
    const folderAtStart = sortKey;
    cancelColumnDrag();
    const clear = () => { columnTarget = null; columnGhost = null; };
    cancelColumnDrag = trackPointerDrag(event, {
      onStart: (move) => { columnGhost = { column, x: move.clientX + 12, y: move.clientY + 12, width }; },
      onMove: (move) => {
        columnGhost = { column, x: Math.min(move.clientX + 12, window.innerWidth - width - 8), y: Math.min(move.clientY + 12, window.innerHeight - 150), width };
        const target = document.elementFromPoint(move.clientX, move.clientY)?.closest<HTMLElement>("[data-list-column]");
        columnTarget = target && row?.contains(target) ? target.dataset.listColumn as ListColumn : null;
      },
      onDrop: () => { if (columnTarget && sortKey === folderAtStart) reorderColumn(column, columnTarget); clear(); },
      onCancel: clear,
    });
  }

  type ColWidths = { name: number | null; type: number; size: number; date: number; modified: number };
  const COL_WIDTHS_STORAGE = "file-list-col-widths";
  const DEFAULT_COL_WIDTHS: ColWidths = { name: null, type: 96, size: 96, date: 168, modified: 168 };

  function loadColWidths(): ColWidths {
    try {
      const raw = localStorage.getItem(COL_WIDTHS_STORAGE);
      if (raw) {
        const saved = JSON.parse(raw);
        const widths = { ...DEFAULT_COL_WIDTHS };
        for (const column of DEFAULT_COLUMNS) {
          const value = saved?.[column];
          if (typeof value === "number" && Number.isFinite(value)) widths[column] = Math.max(column === "name" ? 80 : 48, value);
        }
        return widths;
      }
    } catch {
      /* ignore malformed storage */
    }
    return { ...DEFAULT_COL_WIDTHS };
  }

  let colWidths = $state<ColWidths>(loadColWidths());

  let minimumWidth = $derived(columns.reduce((sum, col) => sum + (colWidths[col] ?? 160), checkboxes ? 56 : 32));
  let gridTemplate = $derived(
    `grid-template-columns: ${checkboxes ? "3.5rem" : "2rem"} ${columns.map(column => column === "name" && colWidths.name == null ? "minmax(160px,1fr)" : `${colWidths[column]}px`).join(" ")}`,
  );

  let cancelResize = () => {};
  $effect(() => () => cancelResize());
  function saveWidths() {
    try { localStorage.setItem(COL_WIDTHS_STORAGE, JSON.stringify(colWidths)); } catch { /* Keep the current widths when storage is unavailable. */ }
  }
  function startResize(col: keyof ColWidths, event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    cancelResize();
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
      window.removeEventListener("pointercancel", onUp);
      saveWidths();
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    cancelResize = onUp;
  }

  let filteredEntries = $derived(showHidden ? entries : entries.filter((entry: DirectoryEntry) => !entry.is_hidden));
  let visibleEntries = $derived(sortColumn ? sortEntries(filteredEntries, sortColumn, sortDir) : filteredEntries);
  let selectedVisible = $derived(visibleEntries.filter((entry: DirectoryEntry) => selectedPaths.has(entry.path)));
  let allSelected = $derived(visibleEntries.length > 0 && selectedVisible.length === visibleEntries.length);
  let externalOver = $derived(drag.overPaneId === paneId);

  // Other lists drop entries on this one through the shared registry.
  $effect(() => {
    if (previewing) return;
    const id = paneId;
    const handler = (entries: DraggedEntry[], destination?: string) => onExternalDrop?.(entries, destination);
    drag.paneDrops.set(id, handler);
    return () => {
      if (drag.paneDrops.get(id) === handler) drag.paneDrops.delete(id);
    };
  });

  let cancelEntryDrag = () => {};
  $effect(() => () => cancelEntryDrag());

  const FOLDER_HOVER_DELAY = 600;
  $effect(() => {
    const path = drag.overEntryPath;
    const folder = path ? visibleEntries.find((entry: DirectoryEntry) => entry.path === path && entry.is_directory) : null;
    if (!folder) return;
    const timer = setTimeout(() => onOpen?.(folder, { newTab: false }), FOLDER_HOVER_DELAY);
    return () => clearTimeout(timer);
  });

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
    const targetEntry = target?.kind === "entry" ? target : null;
    const toPane = target !== null && target.kind !== "favorites" && target.paneId !== paneId;
    const toFolder =
      targetEntry != null &&
      drag.paneEntryFolders.get(targetEntry.paneId)?.(targetEntry.path) === true &&
      !group.some((dragged) => dragged.path === targetEntry.path);
    drag.favoriteDropAt = toFavorites ? target.index : null;
    drag.overPaneId = target && target.kind !== "favorites" && (toPane || toFolder) ? target.paneId : null;
    drag.overEntryPath = toFolder ? targetEntry.path : null;
    drag.ghost = {
      x: event.clientX,
      y: event.clientY,
      name: entry.name,
      icon: entryIcon(entry),
      action: toFavorites ? "favorite" : toPane ? "copy" : toFolder ? "move" : null,
      count: group.length,
    };
  }

  function dropEntry(event: PointerEvent, entry: DirectoryEntry) {
    const target = dropTargetAt(event.clientX, event.clientY);
    const paths = draggedPaths(entry);
    const entries = [...drag.entries];
    endEntryDrag();
    if (!target) return;
    if (target.kind === "favorites") {
      if (paths.length === 1 && canFavorite(entry)) drag.favoritesDrop?.(entry.path, target.index);
    } else if (target.kind === "entry") {
      const isFolder = drag.paneEntryFolders.get(target.paneId)?.(target.path) === true;
      if (isFolder && !paths.includes(target.path)) drag.paneDrops.get(target.paneId)?.(entries, target.path);
      else if (target.paneId === paneId && paths.length === 1) reorder(entry.path, target.path);
    } else if (target.paneId !== paneId) {
      drag.paneDrops.get(target.paneId)?.(entries);
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
  let scrollMargin = $derived(view === "list" ? insetTop + 32 : insetTop + GRID_TOP);

  const rows = createRowVirtualizer({
    count: () => (view === "list" ? visibleEntries.length : gridRowCount),
    estimateSize: () => (view === "list" ? (visibleEntries[0]?.relative_path != null ? SEARCH_ROW_H : LIST_ROW_H) : GRID_ROW_H),
    scrollMargin: () => scrollMargin,
    scrollPaddingStart: () => insetTop + (view === "list" ? 32 : 0),
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

  // A streamed network listing carries names only; ask for the details of what is shown.
  const detailPaths = $derived(rows.virtualItems.flatMap((row) =>
    view === "grid" ? gridRowEntries(row.index) : [visibleEntries[row.index]],
  ).filter((entry): entry is DirectoryEntry => !!entry && !entry.inner_path &&
    entry.modified == null && entry.created == null && entry.size == null)
    .map((entry) => entry.path).join("\0"));

  $effect(() => {
    if (previewing || !detailPaths) return;
    untrack(() => onNeedDetails)?.(detailPaths.split("\0"));
  });

  let downloadSnapshots = $state<Record<string, DownloadSnapshot>>({});
  // Only inspect rendered local items. No directory scans or browser history access.
  const downloadPaths = $derived(rows.virtualItems.flatMap((row) =>
    view === "grid" ? gridRowEntries(row.index) : [visibleEntries[row.index]],
  ).filter((entry): entry is DirectoryEntry => !!entry && !entry.inner_path && canDragOut(entry) &&
    isPartialDownload(entry.name))
    .map((entry) => entry.path).slice(0, 512).join("\0"));

  $effect(() => {
    if (previewing) return;
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

<Resizable.PaneGroup direction="horizontal" autoSaveId={previewing ? undefined : "preview-panel"} class="min-h-0 min-w-0 flex-1">
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
      <!-- Blank-space click to deselect is a mouse-only convenience, like Finder. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
      <div
        bind:this={scrollEl}
        bind:clientWidth={containerW}
        data-file-list
        data-pane-id={previewing ? undefined : paneId}
        data-key-scope={previewing ? undefined : "list"}
        tabindex={previewing ? undefined : 0}
        onclick={previewing ? undefined : handleBlankClick}
        onscroll={() => scrollEl && onScroll?.(scrollEl.scrollTop)}
        class="list-scroll min-h-0 min-w-0 flex-1 overflow-auto px-2 pb-2 [scrollbar-gutter:stable]"
        class:with-header={view === "list"}
        class:remote-drop-active={externalOver}>
      {#if view === "list"}
        <!-- Glass header floats over the scroll area, so rows stay visible, blurred, behind it. -->
        <div
          role="row"
          aria-rowindex={1}
          style={`${gridTemplate}; min-width: ${minimumWidth}px`}
          class="list-header sticky z-10 grid h-8 items-center text-[11px] font-semibold uppercase tracking-wide text-[#9c9895]">
          <div role="columnheader" class="flex items-center px-2">
            <!-- First click turns on checkbox selection; after that it selects or deselects everything. -->
            <SelectionCheckbox
              checked={checkboxes && allSelected}
              indeterminate={checkboxes && !allSelected && selectedVisible.length > 0}
              label={!checkboxes ? "Turn on multiple selection" : allSelected ? "Deselect all" : "Select all"}
              onToggle={() => (checkboxes ? onToggleAll?.() : onEnableCheckboxes?.())} />
          </div>
          {#each columns as column (column)}
          <div role="columnheader" data-list-column={column} aria-sort={sortColumn === column ? sortDir === "asc" ? "ascending" : "descending" : "none"} class="relative flex h-full items-center px-2" class:column-source={columnGhost?.column === column}
            class:column-target-before={columnTarget === column && columnGhost && columns.indexOf(columnGhost.column) > columns.indexOf(column)}
            class:column-target-after={columnTarget === column && columnGhost && columns.indexOf(columnGhost.column) < columns.indexOf(column)}>
            <button type="button" class="flex cursor-grab items-center gap-1 uppercase hover:text-[#e8e5e2] active:cursor-grabbing" class:ml-auto={column !== "name" && column !== "type"}
              onpointerdown={(event) => startColumnDrag(event, column)}
              onclick={() => toggleSort(column)}
              onkeydown={(event) => {
                if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
                  event.preventDefault(); event.stopPropagation();
                  const target = columns[columns.indexOf(column) + (event.key === "ArrowLeft" ? -1 : 1)];
                  if (target) reorderColumn(column, target);
                }
              }}
              title="Drag to reorder · Alt + Left/Right to move" aria-label={`Sort by ${COLUMN_LABELS[column]}`}>
              {COLUMN_LABELS[column]} {@render sortIcon(column)}
            </button>
            <!-- A focusable separator is the ARIA pattern for a keyboard-operated resize handle. -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div role="separator" tabindex="0" aria-orientation="vertical" aria-valuemin={column === "name" ? 80 : 48} aria-valuenow={colWidths[column] ?? 160} aria-label={`Resize ${COLUMN_LABELS[column]} column`}
              onkeydown={(event) => {
                if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
                event.preventDefault(); event.stopPropagation();
                const width = (event.currentTarget as HTMLElement).parentElement?.getBoundingClientRect().width ?? 160;
                colWidths[column] = Math.max(column === "name" ? 80 : 48, width + (event.key === "ArrowLeft" ? -10 : 10));
                saveWidths();
              }} class="column-resize" onpointerdown={(event) => startResize(column, event)}></div>
          </div>
          {/each}
        </div>
      {/if}
        {#if view === "list"}
          {#if visibleEntries.length === 0}
            <p class="p-4 text-center text-[13px] text-[#9c9895]">This folder is empty.</p>
          {:else}
            <div role="rowgroup" class="relative" style="height: {rows.totalSize}px;">
              {#each rows.virtualItems as v (visibleEntries[v.index]?.path ?? v.key)}
                {@const entry = visibleEntries[v.index]}
                {#if entry}
                  {@const selected = selectedPaths.has(entry.path)}
                  <ListItem
                    {entry}
                    downloadSnapshot={downloadSnapshots[entry.path]}
                    view="list"
                    {columns}
                    draggedColumn={columnGhost?.column}
                    rowIndex={v.index + 2}
                    zebra={v.index % 2 === 1}
                    {selected}
                    focused={entry.path === focusPath}
                    joinPrev={selected && v.index > 0 && selectedPaths.has(visibleEntries[v.index - 1].path)}
                    joinNext={selected && v.index + 1 < visibleEntries.length && selectedPaths.has(visibleEntries[v.index + 1].path)}
                    {checkboxes}
                    revealDelay={revealDelay(v.index)}
                    renaming={entry.path === renamingPath}
                    style="position: absolute; top: 0; left: 0; width: 100%; min-width: {minimumWidth}px; height: {v.size}px; transform: translateY({v.start - scrollMargin}px); {gridTemplate}"
                    {onItemClick}
                    {onToggle}
                    {onOpen}
                    {onRename}
                    {onRenameCancel}
                    {onContextMenu}
                    dropTarget={entry.path === drag.overEntryPath}
                    pasted={pastedPaths.has(entry.path)}
                    {searchQuery}
                    onPointerDown={previewing ? undefined : startEntryDrag} />
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
                    onPointerDown={previewing ? undefined : startEntryDrag} />
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
{#if columnGhost}
  <div class="column-ghost" aria-hidden="true" style={`width: ${columnGhost.width}px; transform: translate3d(${columnGhost.x}px, ${columnGhost.y}px, 0)`}>
    <div class="ghost-title"><svg width="10" height="14" viewBox="0 0 10 14" fill="currentColor"><circle cx="3" cy="3" r="1"/><circle cx="7" cy="3" r="1"/><circle cx="3" cy="7" r="1"/><circle cx="7" cy="7" r="1"/><circle cx="3" cy="11" r="1"/><circle cx="7" cy="11" r="1"/></svg>{COLUMN_LABELS[columnGhost.column]}</div>
    {#each visibleEntries.slice(0, 3) as entry}<div class="ghost-cell">{columnPreview(entry, columnGhost.column)}</div>{/each}
  </div>
{/if}

<style>
  .column-resize { position: absolute; right: -5px; top: 0; width: 11px; height: 100%; cursor: col-resize; touch-action: none; z-index: 1; }
  .column-resize::after { content: ""; position: absolute; left: 5px; top: 4px; bottom: 4px; width: 1px; background: rgb(232 229 226 / 28%); }
  .column-resize:focus-visible { outline: 1px solid #70b7ff; border-radius: 3px; }
  .column-resize:hover::after { background: #70b7ff; width: 2px; }
  .column-source { opacity: 0.35; }
  .column-target-before { box-shadow: inset 2px 0 #70b7ff; }
  .column-target-after { box-shadow: inset -2px 0 #70b7ff; }
  .column-ghost { position: fixed; top: 0; left: 0; z-index: 40; pointer-events: none; padding: 4px; border-radius: 10px; background: #34312f; color: #e8e5e2; box-shadow: 0 12px 36px rgb(0 0 0 / 18%), inset 0 0 0 1px rgb(160 204 255 / 28%); }
  .ghost-title { display: flex; align-items: center; gap: 7px; height: 30px; padding: 0 9px; border-radius: 6px 6px 0 0; background: #45413d; color: #c0dfff; font-size: 11px; font-weight: 600; text-transform: uppercase; }
  .ghost-cell { padding: 7px 9px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; background: #2b2826; box-shadow: inset 0 1px rgb(255 255 255 / 5%); }
  .ghost-cell:last-child { border-radius: 0 0 6px 6px; }
  /* `--content-overlap` is the toolbar height the pane slides under (0 when the tab bar is open). */
  .list-scroll {
    padding-top: var(--content-overlap, 0px);
    scroll-padding-top: var(--content-overlap, 0px);
  }
  .list-scroll.with-header {
    padding-top: var(--content-overlap, 0px);
    scroll-padding-top: var(--content-overlap, 0px);
  }

  .list-header {
    top: 0;
    isolation: isolate;
    box-shadow: inset 0 -1px 0 rgb(255 255 255 / 7%);

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
