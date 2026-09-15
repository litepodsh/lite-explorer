<script lang="ts" module>
  export type DirectoryEntry = { name: string; path: string; is_directory: boolean; is_hidden: boolean; size?: number; opened_at?: number; created?: number; kind?: "bucket" | "share" };
</script>

<script lang="ts">
  import FileIcon from "@lucide/svelte/icons/file";
  import { entryType } from "./sort.js";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import { formatSize, formatDate } from "$lib/components/custom/preview/format.js";

  let {
    entry,
    view = "list",
    selected = false,
    renaming = false,
    rowIndex,
    onSelect,
    onOpen,
    onPointerDown,
    dropTarget = false,
    pasted = false,
    onRename,
    onRenameCancel,
    onContextMenu,
    style = "",
  } = $props<{
    entry: DirectoryEntry;
    view?: "list" | "grid";
    selected?: boolean;
    renaming?: boolean;
    rowIndex?: number;
    onSelect?: (entry: DirectoryEntry) => void;
    onOpen?: (entry: DirectoryEntry, options?: { newTab: boolean }) => void;
    /** Starts a pointer drag of the item. */
    onPointerDown?: (event: PointerEvent, entry: DirectoryEntry) => void;
    /** A dragged item from the same list is over this one. */
    dropTarget?: boolean;
    pasted?: boolean;
    onRename?: (oldPath: string, newName: string) => void;
    onRenameCancel?: () => void;
    onContextMenu?: (entry: DirectoryEntry) => void;
    style?: string;
  }>();

  let editName = $state("");
  let wasRenaming = false;
  $effect(() => {
    if (renaming && !wasRenaming) editName = entry.name;
    wasRenaming = renaming;
  });

  // A normal click previews the file, but we wait before firing `onSelect` so a
  // second click can cancel it as a double-click (which opens instead). Right-click
  // never previews: it only opens the context menu.
  const PREVIEW_CLICK_DELAY = 250;
  let clickTimer: ReturnType<typeof setTimeout> | null = null;

  function clearClickTimer() {
    if (clickTimer !== null) {
      clearTimeout(clickTimer);
      clickTimer = null;
    }
  }

  function handleClick() {
    clearClickTimer();
    clickTimer = setTimeout(() => {
      clickTimer = null;
      onSelect?.(entry);
    }, PREVIEW_CLICK_DELAY);
  }

  function handleDblClick(event: MouseEvent) {
    clearClickTimer();
    onOpen?.(entry, { newTab: event.metaKey || event.ctrlKey });
  }

  function focusRename(node: HTMLInputElement) {
    node.focus();
    const full = entry.name;
    if (entry.is_directory) {
      node.select();
    } else {
      const dot = full.lastIndexOf(".");
      const end = dot > 0 ? dot : full.length;
      node.setSelectionRange(0, end);
    }
  }

  function commitRename() {
    const value = editName.trim();
    if (value === "" || value === entry.name) onRenameCancel?.();
    else onRename?.(entry.path, value);
  }
</script>

{#if view === "list"}
  <div
    role="row"
    style={style}
    class="grid h-9 cursor-default items-center border-b border-[#3a3734]/60 text-[13px] text-[#e8e5e2] hover:bg-[#353230] {selected ? 'bg-[#3b3836] text-blue-400' : ''} {dropTarget ? 'entry-drop-target' : ''} {pasted ? 'entry-pasted' : ''}"
    aria-label={entry.name}
    aria-selected={selected}
    aria-rowindex={rowIndex}
    tabindex="-1"
    data-entry-path={entry.path}
    onclick={handleClick}
    onkeydown={(event) => {
      if (event.key === " ") onSelect?.(entry);
    }}
    onpointerdown={(event) => onPointerDown?.(event, entry)}
    oncontextmenu={() => {
      // Don't preventDefault: the surrounding ContextMenu.Trigger ignores events that
      // are already default-prevented, so the menu would never open on an item.
      clearClickTimer();
      onContextMenu?.(entry);
    }}
    ondblclick={handleDblClick}>
    <div role="gridcell" class="px-2">{#if entry.kind === "share"}<HardDriveIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else if entry.is_directory}<FolderIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else}<FileIcon class="size-[17px] text-[#aaa5a1] stroke-[1.7]" />{/if}</div>
    <div role="gridcell" class="truncate px-2">
      {#if renaming}
        <input
          bind:value={editName}
          use:focusRename
          class="w-full rounded-sm border border-[#0a84ff]/60 bg-[#1f1d1b] px-1 text-[#e8e5e2] outline-none"
          onclick={(event) => event.stopPropagation()}
          onkeydown={(event) => {
            if (event.key === "Enter") commitRename();
            else if (event.key === "Escape") onRenameCancel?.();
          }}
          onblur={commitRename} />
      {:else}
        {entry.name}
      {/if}
    </div>
    <div role="gridcell" class="px-2 text-xs text-[#9c9895]">{entryType(entry)}</div>
    <div role="gridcell" class="px-2 text-right text-xs tabular-nums text-[#9c9895]">{entry.is_directory || entry.size == null ? "—" : formatSize(entry.size)}</div>
    <div role="gridcell" class="px-2 text-right text-xs tabular-nums text-[#9c9895]">{entry.created == null ? "—" : formatDate(entry.created)}</div>
  </div>
{:else}
  <button
    class="flex h-24 w-full min-w-0 flex-col items-center justify-center gap-1 rounded-md border-0 bg-transparent p-3 text-center text-[13px] text-[#e8e5e2] hover:bg-[#353230] {selected ? 'bg-[#3b3836] text-blue-400' : ''} {dropTarget ? 'entry-drop-target' : ''}"
    aria-label={entry.name}
    aria-pressed={selected}
    data-entry-path={entry.path}
    onclick={handleClick}
    onpointerdown={(event) => onPointerDown?.(event, entry)}
    oncontextmenu={() => {
      // Don't preventDefault: the surrounding ContextMenu.Trigger ignores events that
      // are already default-prevented, so the menu would never open on an item.
      clearClickTimer();
      onContextMenu?.(entry);
    }}
    ondblclick={handleDblClick}>
    {#if entry.kind === "share"}<HardDriveIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else if entry.is_directory}<FolderIcon class="size-[17px] text-blue-400 stroke-[1.7]" />{:else}<FileIcon class="size-[17px] text-[#aaa5a1] stroke-[1.7]" />{/if}
    {#if renaming}
      <input
        bind:value={editName}
        use:focusRename
        class="max-w-full rounded-sm border border-[#0a84ff]/60 bg-[#1f1d1b] px-1 text-[#e8e5e2] outline-none"
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => {
          if (event.key === "Enter") commitRename();
          else if (event.key === "Escape") onRenameCancel?.();
        }}
        onblur={commitRename} />
    {:else}
      <span class="max-w-full truncate">{entry.name}</span>
    {/if}
  </button>
{/if}
