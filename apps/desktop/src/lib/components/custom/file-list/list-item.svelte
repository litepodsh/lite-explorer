<script lang="ts" module>
  export type DirectoryEntry = { name: string; path: string; is_directory: boolean; is_hidden: boolean; size?: number; sizeComplete?: boolean; opened_at?: number; created?: number; kind?: "bucket" | "share"; relative_path?: string; snippet?: string; inner_path?: string };
</script>

<script lang="ts">
  import DownloadIndicator from "$lib/transfers/download-indicator.svelte";
  import type { DownloadSnapshot } from "$lib/transfers/download-progress.js";
  import EntryIcon from "$lib/file-icons/entry-icon.svelte";
  import { entryType } from "./sort.js";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import { canDragOut } from "$lib/file-drag/native-drag.js";
  import { formatSize, formatDate } from "$lib/components/custom/preview/format.js";
  import { isPrimaryModifier } from "$lib/state/platform.svelte.js";
  import SelectionCheckbox from "./selection-checkbox.svelte";
  import { checkboxReveal } from "./checkbox-reveal.js";
  let {
    entry,
    downloadSnapshot,
    view = "list",
    selected = false,
    focused = false,
    joinPrev = false,
    joinNext = false,
    checkboxes = false,
    revealDelay = 0,
    renaming = false,
    rowIndex,
    zebra = false,
    onItemClick,
    onToggle,
    onOpen,
    onPointerDown,
    dropTarget = false,
    pasted = false,
    onRename,
    onRenameCancel,
    onContextMenu,
    style = "",
    searchQuery = "",
  } = $props<{
    entry: DirectoryEntry;
    downloadSnapshot?: DownloadSnapshot;
    view?: "list" | "grid";
    selected?: boolean;
    /** Keyboard focus is on this item. */
    focused?: boolean;
    /** The item before or after is selected too, so the highlight joins into one block. */
    joinPrev?: boolean;
    joinNext?: boolean;
    /** Shows a selection checkbox. */
    checkboxes?: boolean;
    /** Stagger (ms) for the checkbox reveal when checkboxes are turned on or off. */
    revealDelay?: number;
    renaming?: boolean;
    rowIndex?: number;
    /** Alternating row shading, based on the visible row index. */
    zebra?: boolean;
    onItemClick?: (entry: DirectoryEntry, modifiers: { primary: boolean; shift: boolean }) => void;
    onToggle?: (entry: DirectoryEntry) => void;
    onOpen?: (entry: DirectoryEntry, options?: { newTab: boolean }) => void;
    /** Starts a pointer drag of the item. */
    onPointerDown?: (event: PointerEvent, entry: DirectoryEntry) => void;
    /** A dragged item from the same list is over this one. */
    dropTarget?: boolean;
    pasted?: boolean;
    onRename?: (oldPath: string, newName: string) => void;
    onRenameCancel?: () => void;
    onContextMenu?: (entry: DirectoryEntry) => void;
    /** Content-search query to highlight inside the snippet. */
    searchQuery?: string;
    style?: string;
  }>();

  // Local files get the OS icon; archive-inner entries are pseudo-paths with no
  // real file to query.
  function usesNativeIcon(entry: DirectoryEntry): boolean {
    return entry.inner_path == null && canDragOut(entry);
  }

  let editName = $state("");
  let wasRenaming = false;
  $effect(() => {
    if (renaming && !wasRenaming) editName = entry.name;
    wasRenaming = renaming;
  });

  // Selection follows the click right away; the controller delays the preview so a
  // double-click can open instead. Right-click never previews: it only opens the context menu.
  function handleClick(event: MouseEvent) {
    onItemClick?.(entry, { primary: isPrimaryModifier(event), shift: event.shiftKey });
  }

  function handleDblClick(event: MouseEvent) {
    if (event.shiftKey) return;
    onOpen?.(entry, { newTab: event.metaKey || event.ctrlKey });
  }

  function focusRename(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // Folder part of a search result's path; empty for results directly in the searched folder.
  const resultFolder = $derived.by(() => {
    const path = entry.relative_path?.replaceAll("\\", "/") ?? "";
    const slash = path.lastIndexOf("/");
    return slash > 0 ? path.slice(0, slash) : "";
  });

  // Splits the snippet around the first case-insensitive query match so it can be highlighted.
  const snippetParts = $derived.by(() => {
    const text = entry.snippet ?? "";
    const lower = text.toLowerCase();
    // Lowercasing can change string length (e.g. "İ"); offsets would then not map back, so skip the highlight.
    const at = searchQuery && lower.length === text.length ? lower.indexOf(searchQuery.toLowerCase()) : -1;
    if (at < 0) return { before: text, match: "", after: "" };
    // Keep the match visible when the line is long: drop leading text far before it.
    const cut = Math.max(0, at - 40);
    return { before: (cut > 0 ? "…" : "") + text.slice(cut, at), match: text.slice(at, at + searchQuery.length), after: text.slice(at + searchQuery.length) };
  });

  function commitRename() {
    const value = editName.trim();
    if (value === "" || value === entry.name) onRenameCancel?.();
    else onRename?.(entry.path, value);
  }
</script>

{#snippet sizeLabel()}
  {#if entry.size != null}
    {#key `${entry.size}:${entry.sizeComplete}`}
      <span class:measured-size={entry.sizeComplete != null} title={entry.sizeComplete === false ? "Partial size · some contents are still unmeasured" : undefined}>{entry.sizeComplete === false ? "≥ " : ""}{formatSize(entry.size)}</span>
    {/key}
  {:else}
    <span title={entry.sizeComplete === false ? "Size not calculated" : undefined}>—</span>
  {/if}
{/snippet}

{#if view === "list"}
  <div
    role="row"
    style={style}
    class="file-row grid h-9 cursor-default items-center text-[13px] text-[#e8e5e2] {dropTarget ? 'entry-drop-target' : ''} {pasted ? 'entry-pasted' : ''}"
    aria-label={entry.name}
    aria-selected={selected}
    aria-rowindex={rowIndex}
    tabindex="-1"
    data-entry-path={entry.path}
    data-selected={selected || undefined}
    data-focused={focused || undefined}
    data-join-prev={joinPrev || undefined}
    data-join-next={joinNext || undefined}
    data-zebra={zebra || undefined}
    onclick={handleClick}
    onkeydown={() => {}}
    onpointerdown={(event) => onPointerDown?.(event, entry)}
    oncontextmenu={() => {
      // Don't preventDefault: the surrounding ContextMenu.Trigger ignores events that
      // are already default-prevented, so the menu would never open on an item.
      onContextMenu?.(entry);
    }}
    ondblclick={handleDblClick}>
    <div role="gridcell" class="flex items-center gap-2 px-2">{#if checkboxes}<span class="flex" transition:checkboxReveal={{ delay: revealDelay }}><SelectionCheckbox checked={selected} label={`Select ${entry.name}`} onToggle={() => onToggle?.(entry)} /></span>{/if}{#if entry.kind === "share"}<HardDriveIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else if entry.is_directory}<FolderIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else}<EntryIcon path={entry.path} name={entry.name} native={usesNativeIcon(entry)} />{/if}</div>
    <div role="gridcell" class="flex min-w-0 items-center gap-2 px-2">
      <span class="min-w-0 flex-1 truncate">
      {#if renaming}
        <span class="relative flex w-full items-center gap-0.5 overflow-hidden rounded-sm border border-[#0a84ff]/60 bg-[#1f1d1b] pr-1 transition-colors duration-300 motion-reduce:transition-none">
          <input
            bind:value={editName}
            use:focusRename
            class="min-w-0 flex-1 bg-transparent px-1 text-[#e8e5e2] outline-none"
            onclick={(event) => event.stopPropagation()}
            onkeydown={(event) => {
              if (event.key === "Enter") commitRename();
              else if (event.key === "Escape") onRenameCancel?.();
            }}
            onblur={commitRename} />
        </span>
      {:else}
        {#if entry.relative_path != null}
          <span class="search-result">
            <span class="search-result-name">{entry.name}</span>
            <span class="search-result-meta">
              {#if resultFolder}<span class="search-result-folder">{resultFolder}</span>{/if}
              {#if entry.inner_path}<span class="search-result-inner" title={entry.inner_path}>{entry.inner_path}</span>{/if}
              {#if entry.snippet}<span class="search-result-snippet">{snippetParts.before}{#if snippetParts.match}<mark>{snippetParts.match}</mark>{/if}{snippetParts.after}</span>{/if}
            </span>
          </span>
        {:else}
          {entry.name}
        {/if}
      {/if}
      </span>
      {#if usesNativeIcon(entry)}<DownloadIndicator path={entry.path} name={entry.name} snapshot={downloadSnapshot} />{/if}
    </div>
    <div role="gridcell" class="px-2 text-xs text-[#9c9895]">{entryType(entry)}</div>
    <div role="gridcell" class="px-2 text-right text-xs tabular-nums text-[#9c9895]">
      {@render sizeLabel()}
    </div>
    <div role="gridcell" class="px-2 text-right text-xs tabular-nums text-[#9c9895]">{entry.created == null ? "—" : formatDate(entry.created)}</div>
  </div>
{:else}
  <button
    class="file-tile relative flex h-24 w-full min-w-0 flex-col items-center justify-center gap-1 rounded-md border-0 bg-transparent p-3 text-center text-[13px] text-[#e8e5e2] {dropTarget ? 'entry-drop-target' : ''} {pasted ? 'entry-pasted' : ''}"
    aria-label={entry.name}
    aria-pressed={selected}
    data-entry-path={entry.path}
    data-selected={selected || undefined}
    data-focused={focused || undefined}
    onclick={handleClick}
    onmousedown={(event) => {
      // Keeps focus off the tile so arrow keys still reach the file list; the rename field still takes it.
      if (!(event.target instanceof Element && event.target.closest("input"))) event.preventDefault();
    }}
    onpointerdown={(event) => onPointerDown?.(event, entry)}
    oncontextmenu={() => {
      // Don't preventDefault: the surrounding ContextMenu.Trigger ignores events that
      // are already default-prevented, so the menu would never open on an item.
      onContextMenu?.(entry);
    }}
    ondblclick={handleDblClick}>
    {#if checkboxes}
      <span class="absolute left-2 top-2 flex" transition:checkboxReveal={{ delay: revealDelay }}>
        <SelectionCheckbox checked={selected} label={`Select ${entry.name}`} onToggle={() => onToggle?.(entry)} />
      </span>
    {/if}
    <span class="flex items-center gap-1.5">
    {#if entry.kind === "share"}<HardDriveIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else if entry.is_directory}<FolderIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else}<EntryIcon path={entry.path} name={entry.name} native={usesNativeIcon(entry)} />{/if}
    {#if usesNativeIcon(entry)}<DownloadIndicator path={entry.path} name={entry.name} snapshot={downloadSnapshot} />{/if}
    </span>
    {#if renaming}
      <span class="relative flex max-w-full items-center gap-0.5 overflow-hidden rounded-sm border border-[#0a84ff]/60 bg-[#1f1d1b] pr-1 transition-colors duration-300 motion-reduce:transition-none">
        <input
          bind:value={editName}
          use:focusRename
          class="min-w-0 flex-1 bg-transparent px-1 text-[#e8e5e2] outline-none"
          onclick={(event) => event.stopPropagation()}
          onkeydown={(event) => {
            if (event.key === "Enter") commitRename();
            else if (event.key === "Escape") onRenameCancel?.();
          }}
          onblur={commitRename} />
      </span>
    {:else}
      <span class="max-w-full truncate">{entry.name}</span>
    {/if}
    {#if entry.sizeComplete != null}
      <span class="text-[10px] tabular-nums text-[#9c9895]">{@render sizeLabel()}</span>
    {/if}
  </button>
{/if}

<style>
  .measured-size {
    display: inline-block;
    animation: size-reveal 240ms cubic-bezier(0.2, 0, 0, 1) both;
  }
  @keyframes size-reveal {
    from { opacity: 0.3; transform: translateY(3px); color: #5cb9ff; }
    to { opacity: 1; transform: translateY(0); }
  }
  @media (prefers-reduced-motion: reduce) {
    .measured-size { animation: none; }
  }

  .file-row {
    --ease: cubic-bezier(0.32, 0.72, 0, 1);
    box-shadow: inset 0 -1px 0 rgb(58 55 52 / 0.6);
    transition:
      transform 240ms cubic-bezier(0.2, 0, 0, 1),
      background-color 140ms var(--ease),
      grid-template-columns 260ms var(--ease);
  }

  .file-row[data-zebra] {
    background: rgb(255 255 255 / 0.03);
  }

  .file-row:hover {
    background: #353230;
  }

  .file-row[data-selected] {
    background: rgb(10 132 255 / 0.18);
    box-shadow: none;
    border-radius: 6px;
  }

  .file-row[data-selected]:hover {
    background: rgb(10 132 255 / 0.25);
  }

  /* Contiguous selected rows read as one block: only its outer corners are rounded. */
  .file-row[data-selected][data-join-prev] {
    border-top-left-radius: 0;
    border-top-right-radius: 0;
  }

  .file-row[data-selected][data-join-next] {
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
    box-shadow: inset 0 -1px 0 rgb(10 132 255 / 0.1);
  }

  /* Keyboard focus that is not selected (moved with Ctrl/Cmd + arrows). */
  .file-row[data-focused]:not([data-selected]),
  .file-tile[data-focused]:not([data-selected]) {
    border-radius: 6px;
    box-shadow: inset 0 0 0 1px rgb(10 132 255 / 0.6);
  }

  .file-tile {
    transition: background-color 140ms cubic-bezier(0.32, 0.72, 0, 1);
  }

  .file-tile:hover {
    background: #353230;
  }

  .file-tile[data-selected] {
    background: rgb(10 132 255 / 0.18);
    box-shadow: inset 0 0 0 1px rgb(10 132 255 / 0.32);
  }

  .file-tile[data-selected]:hover {
    background: rgb(10 132 255 / 0.25);
  }

  /* Like Explorer, the selection turns gray while its pane is not the active one. */
  :global(.file-pane:not(.active)) .file-row[data-selected],
  :global(.file-pane:not(.active)) .file-tile[data-selected] {
    background: #3b3836;
  }

  :global(.file-pane:not(.active)) .file-tile[data-selected] {
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.06);
  }

  @media (prefers-reduced-motion: reduce) {
    .file-row,
    .file-tile {
      transition: none;
    }
  }
</style>
