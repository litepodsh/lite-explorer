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

  let { path = "", network = false, entries = [], showFps = false, activity = null } = $props<{
    path?: string;
    /** The path is inside a network location, shown as its server address. */
    network?: boolean;
    entries?: DirectoryEntry[];
    showFps?: boolean;
    /** Background work to surface, like a folder scan. Shown with a spinner while set. */
    activity?: string | null;
  }>();

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const targetCount = $derived(entries.length);
  const targetBytes = $derived(
    entries.reduce((sum: number, entry: DirectoryEntry) => sum + (entry.size ?? 0), 0),
  );

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
  {#if activity}
    <span class="ml-auto inline-flex shrink-0 items-center gap-1.5 tabular-nums text-[#9c9895]" aria-live="polite">
      <LoaderCircleIcon class="size-3 animate-spin text-[#0a9bff] motion-reduce:animate-none" />{activity}
    </span>
  {/if}
  {#if targetCount > 0}
    <span
      class="{activity ? 'pl-3' : 'ml-auto'} shrink-0 tabular-nums text-[#9c9895]"
      aria-live="polite"
      aria-label={`${targetCount} items${targetBytes > 0 ? `, ${formatSize(targetBytes)}` : ""}`}>
      {Math.round(count.current)} item{targetCount === 1 ? "" : "s"}{#if targetBytes > 0}<span
          class="px-1 text-[#5c5854]">·</span
        >{formatSize(bytes.current)}{/if}
    </span>
  {/if}
  {#if showFps}
    <span class={targetCount > 0 || activity ? "" : "ml-auto"}><FpsMeter /></span>
  {/if}
</footer>
