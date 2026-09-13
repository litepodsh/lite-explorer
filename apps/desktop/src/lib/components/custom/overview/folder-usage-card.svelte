<script lang="ts">
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import HouseIcon from "@lucide/svelte/icons/house";
  import { flip } from "svelte/animate";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import { UsageBar } from "$lib/components/custom/usage-bar/index.js";
  import type { FolderUsage, FolderUsageEntry } from "./types.js";
  import { formatAge, summarizeFolders } from "./usage.js";

  let {
    usage,
    scanning,
    scannedBytes,
    currentFolder,
    error,
    showHidden,
    nowSeconds,
    onRefresh,
    onOpen,
  }: {
    usage: FolderUsage | null;
    scanning: boolean;
    scannedBytes: number;
    currentFolder: string | null;
    error: string;
    showHidden: boolean;
    nowSeconds: number;
    onRefresh: () => void;
    onOpen: (entry: FolderUsageEntry) => void;
  } = $props();

  const rootName = $derived(usage?.root.split("/").filter(Boolean).at(-1) ?? "");
  const hasData = $derived(!!usage && usage.entries.length > 0);
  const totalBytes = $derived(usage ? Math.max(usage.total_bytes, scanning && usage.scanned_at === null ? scannedBytes : 0) : 0);
  const summary = $derived(summarizeFolders(usage?.entries ?? [], totalBytes, { showHidden }));
  const largest = $derived(summary.top[0]?.bytes ?? 0);
</script>

<article class="overflow-hidden rounded-xl border border-[#3a3734] bg-[#2d2a28]">
  <header class="flex items-center gap-3 border-b border-[#3a3734] px-4 py-3.5">
    <HouseIcon class="size-7 shrink-0 stroke-[1.4] text-blue-400" />
    <div class="min-w-0">
      <h2 class="text-[15px] font-semibold text-[#f2f1f0]">What’s using your home folder</h2>
      <p class="mt-0.5 truncate text-xs text-[#9c9895]">{rootName || "Home"}</p>
    </div>
    <div class="ml-auto flex shrink-0 items-center gap-2 text-xs tabular-nums text-[#9c9895]" aria-live="polite">
      {#if scanning}
        <LoaderCircleIcon class="size-3.5 animate-spin text-[#0a9bff] motion-reduce:animate-none" />
        <span title={currentFolder ?? undefined}>Refreshing… {formatSize(scannedBytes)}</span>
      {:else}
        {#if usage?.scanned_at}<span>Updated {formatAge(usage.scanned_at, nowSeconds)}</span>{/if}
        <button
          class="grid size-7 place-items-center rounded-md border-0 bg-transparent text-[#9c9895] hover:bg-[#353230] hover:text-[#e8e5e2] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff]"
          aria-label="Refresh folder sizes"
          title="Refresh folder sizes"
          onclick={onRefresh}><RefreshCwIcon class="size-3.5" /></button>
      {/if}
    </div>
  </header>

  <div class="grid p-1.5">
    {#if error}
      <p class="px-2.5 py-2 text-xs text-[#ff8a80]">{error}</p>
    {/if}
    {#if hasData}
      {#each summary.top as entry, index (entry.path)}
        <button
          animate:flip={{ duration: 320 }}
          class="grid grid-cols-[18px_minmax(0,1fr)_76px] items-center gap-x-2.5 gap-y-1.5 rounded-[7px] border-0 bg-transparent px-2.5 py-2 text-left hover:bg-[#353230] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff]"
          title={entry.path}
          onclick={() => onOpen(entry)}>
          <FolderIcon class="size-4 stroke-[1.8] {entry.is_hidden ? 'text-[#67635f]' : 'text-blue-400'}" />
          <span class="truncate text-[13px] {entry.is_hidden ? 'text-[#9c9895]' : 'text-[#d9d6d3]'}">{entry.name}</span>
          <span class="text-right font-mono text-xs tabular-nums text-[#d9d6d3]">{formatSize(entry.bytes)}</span>
          <UsageBar
            class="col-start-2 col-end-4"
            height={3}
            gap={0}
            delay={index * 45}
            total={largest}
            segments={[{ id: entry.path, value: entry.bytes, color: entry.is_hidden ? "#7b7773" : undefined }]} />
        </button>
      {/each}
      {#if summary.restBytes > 0}
        <div class="grid grid-cols-[18px_minmax(0,1fr)_76px] items-center gap-x-2.5 gap-y-1.5 px-2.5 py-2">
          <EllipsisIcon class="size-4 text-[#67635f]" />
          <span class="truncate text-[13px] text-[#9c9895]"
            >Other files and folders{summary.restHasHidden ? ", including hidden" : ""}</span>
          <span class="text-right font-mono text-xs tabular-nums text-[#9c9895]">{formatSize(summary.restBytes)}</span>
          <UsageBar
            class="col-start-2 col-end-4"
            height={3}
            gap={0}
            delay={summary.top.length * 45}
            total={Math.max(largest, summary.restBytes)}
            segments={[{ id: "rest", value: summary.restBytes, color: "#4a4643" }]} />
        </div>
      {/if}
    {:else if scanning}
      <p class="px-2.5 pt-2 pb-1 text-xs text-[#9c9895]">Measuring your home folder for the first time. This can take a minute.</p>
      {#each [72, 58, 44, 40, 26, 14] as width (width)}
        <div class="grid grid-cols-[18px_minmax(0,1fr)_76px] items-center gap-x-2.5 gap-y-2 px-2.5 py-2.5" aria-hidden="true">
          <span class="size-4 rounded bg-[#353230]"></span>
          <span class="h-2.5 animate-pulse rounded bg-[#353230] motion-reduce:animate-none" style:width="{width}%"></span>
          <span class="ml-auto h-2.5 w-12 animate-pulse rounded bg-[#353230] motion-reduce:animate-none"></span>
          <span class="col-start-2 col-end-4 h-[3px] rounded-full bg-[#1a1918]"></span>
        </div>
      {/each}
    {:else if !error}
      <p class="px-2.5 py-3 text-xs text-[#9c9895]">No folder sizes yet. Refresh to measure your home folder.</p>
    {/if}
  </div>
</article>
