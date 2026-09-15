<script lang="ts">
  import CheckIcon from "@lucide/svelte/icons/check";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import NetworkIcon from "@lucide/svelte/icons/network";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import { Tween } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
  import FpsMeter from "./fps-meter.svelte";
  import { openCommandPalette } from "$lib/state/command-palette.svelte";
  import SearchIcon from "@lucide/svelte/icons/search";

  let { path = "", network = false, entries = [], selectedEntries = [], showFps = false, activity = null, keyboardMode = "standard", visual = false, pendingKeys = "" } = $props<{
    path?: string;
    /** The path is inside a network location, shown as its server address. */
    network?: boolean;
    entries?: DirectoryEntry[];
    /** Selected entries; with any, the count and size describe them instead of the whole folder. */
    selectedEntries?: DirectoryEntry[];
    showFps?: boolean;
    /** Background work to surface, like a folder scan. Shown with a spinner while set. */
    activity?: string | null;
    keyboardMode?: "standard" | "yazi";
    /** Yazi visual mode is on in this pane. */
    visual?: boolean;
    /** Keys of a chord being typed, like `g`. */
    pendingKeys?: string;
  }>();

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const selecting = $derived(selectedEntries.length > 0);
  const targetCount = $derived(entries.length);
  const targetBytes = $derived(
    (selecting ? selectedEntries : entries).reduce((sum: number, entry: DirectoryEntry) => sum + (entry.size ?? 0), 0),
  );
  const selectedTween = new Tween(0, { duration: 420, easing: cubicOut });
  $effect(() => {
    selectedTween.target = selectedEntries.length;
  });

  const count = new Tween(0, { duration: 650, easing: cubicOut });
  const bytes = new Tween(0, { duration: 650, easing: cubicOut });

  $effect(() => {
    count.target = targetCount;
    bytes.target = targetBytes;
  });

  async function copyPath() {
    if (!path) return;
    await navigator.clipboard.writeText(path);
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 1500);
  }
</script>

<footer
  class="group flex h-7 shrink-0 items-center gap-1.5 border-t border-[#3a3734] bg-[#242220] px-3 text-[11px] text-[#9c9895]"
  aria-label="Current path">
  {#if network}
    <NetworkIcon class="size-3.5 shrink-0 text-blue-400 stroke-[1.7]" />
  {:else}
    <FolderIcon class="size-3.5 shrink-0 text-blue-400 stroke-[1.7]" />
  {/if}
  <span class="min-w-0 truncate select-text" title={path}>{path || "No location selected"}</span>
  <button
    class="grid size-5 shrink-0 place-items-center rounded border-0 bg-transparent text-[#9c9895] opacity-0 transition-opacity hover:bg-[#353230] hover:text-[#e8e5e2] focus-visible:opacity-100 group-hover:opacity-100"
    aria-label="Go to Folder"
    title="Go to Folder (⇧⌘P)"
    onclick={openCommandPalette}>
    <SearchIcon class="size-3.5" />
  </button>
  {#if path}
    <button
      class="grid size-5 shrink-0 place-items-center rounded border-0 bg-transparent text-[#9c9895] opacity-0 transition-opacity hover:bg-[#353230] hover:text-[#e8e5e2] focus-visible:opacity-100 group-hover:opacity-100 {copied ? 'opacity-100 text-green-400' : ''}"
      aria-label={copied ? "Path copied" : "Copy path"}
      title={copied ? "Copied" : "Copy path"}
      onclick={copyPath}>
      {#if copied}<CheckIcon class="size-3.5" />{:else}<CopyIcon class="size-3.5" />{/if}
    </button>
  {/if}
  {#if keyboardMode === "yazi" || visual || pendingKeys}
    <span class="ml-2 inline-flex shrink-0 items-center gap-1">
      {#if keyboardMode === "yazi"}<span class="key-pill">YAZI</span>{/if}
      {#if visual}<span class="key-pill key-pill-visual">VISUAL</span>{/if}
      {#if pendingKeys}<span class="key-pill key-pill-pending" aria-live="polite">{pendingKeys} …</span>{/if}
    </span>
  {/if}
  {#if activity}
    <span class="ml-auto inline-flex shrink-0 items-center gap-1.5 tabular-nums text-[#9c9895]" aria-live="polite">
      <LoaderCircleIcon class="size-3 animate-spin text-[#0a9bff] motion-reduce:animate-none" />{activity}
    </span>
  {/if}
  {#if targetCount > 0}
    <span
      class="{activity ? 'pl-3' : 'ml-auto'} shrink-0 tabular-nums text-[#9c9895]"
      aria-live="polite"
      aria-label={`${selecting ? `${selectedEntries.length} of ` : ""}${targetCount} items${selecting ? " selected" : ""}${targetBytes > 0 ? `, ${formatSize(targetBytes)}` : ""}`}>
      {#if selecting}<span class="text-[#5cb9ff]">{Math.round(selectedTween.current)}</span> of {/if}{Math.round(count.current)} item{targetCount === 1 ? "" : "s"}{#if selecting} selected{/if}{#if targetBytes > 0}<span
          class="px-1 text-[#5c5854]">·</span
        >{formatSize(bytes.current)}{/if}
    </span>
  {/if}
  {#if showFps}
    <span class={targetCount > 0 || activity ? "" : "ml-auto"}><FpsMeter /></span>
  {/if}
</footer>

<style>
  .key-pill {
    padding: 0 6px;
    border-radius: 999px;
    background: rgb(255 255 255 / 6%);
    color: #a8a4a1;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    line-height: 16px;
  }
  .key-pill-visual {
    background: rgb(10 155 255 / 18%);
    color: #5cb9ff;
  }
  .key-pill-pending {
    background: rgb(255 184 76 / 16%);
    color: #ffc76b;
    letter-spacing: 0.02em;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  }
</style>
