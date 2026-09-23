<script lang="ts">
  import CheckIcon from "@lucide/svelte/icons/check";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import NetworkIcon from "@lucide/svelte/icons/network";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import { Tween, prefersReducedMotion } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import { formatCount, formatSize } from "$lib/components/custom/preview/format.js";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
  import FpsMeter from "./fps-meter.svelte";
  import { openCommandPaletteWith } from "$lib/state/command-palette.svelte";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { isRemotePath } from "$lib/remote/remote-locations.js";
  import { isNetworkPath } from "$lib/remote/network-locations.js";
  import { breadcrumbSegments, collapseSegments } from "./breadcrumb.js";

  let { path = "", location = "", onNavigate, network = false, entries = [], selectedEntries = [], showFps = false, activity = null, keyboardMode = "standard", visual = false, pendingKeys = "", sizeScanning = false, sizeScanMessage = "", onCancelSizeScan } = $props<{
    sizeScanning?: boolean;
    sizeScanMessage?: string;
    onCancelSizeScan?: () => void;
    /** The path as shown, like an `s3://bucket` URI or a share's server address. */
    path?: string;
    /** The internal path of the listing that `path` describes; breadcrumb segments open folders of it. */
    location?: string;
    onNavigate?: (path: string) => void;
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
  const sizesPartial = $derived((selecting ? selectedEntries : entries).some((entry: DirectoryEntry) =>
    entry.sizeComplete === false || (entry.is_directory && entry.size == null)));
  const selectedTween = new Tween(0, { duration: () => prefersReducedMotion.current ? 0 : 420, easing: cubicOut });
  $effect(() => {
    selectedTween.target = selectedEntries.length;
  });

  const count = new Tween(0, { duration: () => prefersReducedMotion.current ? 0 : 650, easing: cubicOut });
  const bytes = new Tween(0, { duration: () => prefersReducedMotion.current ? 0 : 650, easing: cubicOut });

  $effect(() => {
    // A folder still streaming in jumps by thousands; counting up to it would never catch up.
    if (Math.abs(targetCount - count.current) > 5_000) count.set(targetCount, { duration: 0 });
    else count.target = targetCount;
    bytes.target = targetBytes;
  });

  const segments = $derived(breadcrumbSegments(location, path));
  const crumbs = $derived(collapseSegments(segments));
  // The palette completes local folders only; S3, SFTP/FTP and unmounted shares can't be typed there.
  const editable = $derived(Boolean(location) && !isRemotePath(location) && !isNetworkPath(location));

  function editPath() {
    const separator = location.includes("\\") && !location.includes("/") ? "\\" : "/";
    openCommandPaletteWith(/[\\/]$/.test(location) ? location : location + separator);
  }

  async function copyPath() {
    if (!path) return;
    await navigator.clipboard.writeText(path);
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 1500);
  }
</script>

{#snippet crumb(segment: { label: string; path: string }, current: boolean)}
  {#if current}
    <span class="crumb crumb-current min-w-0 truncate select-text" aria-current="location">{segment.label}</span>
  {:else}
    <button type="button" class="crumb max-w-48 shrink-0 truncate" title={segment.path} onclick={() => onNavigate?.(segment.path)}>{segment.label}</button>
    <span class="crumb-separator" aria-hidden="true">›</span>
  {/if}
{/snippet}

<footer
  class="group relative flex h-7 shrink-0 items-center gap-1.5 border-t border-[#3a3734] bg-[#242220] px-3 text-[11px] text-[#9c9895]"
  aria-label="Current path">
  {#if sizeScanning}<span class="size-scan-track" aria-hidden="true"><span></span></span>{/if}
  {#if network}
    <NetworkIcon class="size-3.5 shrink-0 text-blue-400 stroke-[1.7]" />
  {:else}
    <FolderIcon class="size-3.5 shrink-0 text-blue-400 stroke-[1.7]" />
  {/if}
  {#if segments.length}
    <nav class="flex min-w-0 items-center overflow-hidden" aria-label="Path" title={path}>
      {#each crumbs.head as segment, index (segment.path)}
        {@render crumb(segment, index === segments.length - 1)}
      {/each}
      {#if crumbs.hidden.length}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger class="crumb shrink-0" aria-label="Show hidden folders">…</DropdownMenu.Trigger>
          <DropdownMenu.Content side="top" class="w-auto max-w-80">
            {#each crumbs.hidden as segment (segment.path)}
              <DropdownMenu.Item onSelect={() => onNavigate?.(segment.path)}>
                <FolderIcon class="text-blue-400" /><span class="truncate">{segment.label}</span>
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
        <span class="crumb-separator" aria-hidden="true">›</span>
      {/if}
      {#each crumbs.tail as segment, index (segment.path)}
        {@render crumb(segment, index === crumbs.tail.length - 1)}
      {/each}
    </nav>
  {:else}
    <span class="min-w-0 truncate">No location selected</span>
  {/if}
  {#if editable}
    <button
      class="grid size-5 shrink-0 place-items-center rounded border-0 bg-transparent text-[#9c9895] opacity-0 transition-opacity hover:bg-[#353230] hover:text-[#e8e5e2] focus-visible:opacity-100 group-hover:opacity-100"
      aria-label="Edit path"
      title="Edit path"
      onclick={editPath}>
      <PencilIcon class="size-3.5" />
    </button>
  {/if}
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
  {#if sizeScanning || sizeScanMessage}
    <span class="size-scan-status ml-auto inline-flex min-w-0 items-center gap-1.5" role="status">
      {#if sizeScanning}
        <LoaderCircleIcon class="size-3 shrink-0 animate-spin text-[#5cb9ff] motion-reduce:animate-none" />
        <span class="truncate">Calculating sizes…</span>
      {:else}
        <span class="truncate" title={sizeScanMessage}>{sizeScanMessage}</span>
      {/if}
    </span>
    {#if sizeScanning}
      <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[#c5c1be] transition-colors hover:bg-white/10 hover:text-white focus-visible:outline focus-visible:outline-1 focus-visible:outline-[#5cb9ff]" onclick={onCancelSizeScan}>Cancel</button>
    {/if}
  {:else if activity}
    <span class="ml-auto inline-flex shrink-0 items-center gap-1.5 tabular-nums text-[#9c9895]" aria-live="polite">
      <LoaderCircleIcon class="size-3 animate-spin text-[#0a9bff] motion-reduce:animate-none" />{activity}
    </span>
  {/if}
  {#if targetCount > 0}
    <span
      class="{activity || sizeScanning || sizeScanMessage ? 'pl-3' : 'ml-auto'} shrink-0 tabular-nums text-[#9c9895]"
      aria-live="polite"
      aria-label={`${selecting ? `${formatCount(selectedEntries.length)} of ` : ""}${formatCount(targetCount)} items${selecting ? " selected" : ""}${targetBytes > 0 ? `, ${sizesPartial ? "at least " : ""}${formatSize(targetBytes)}` : ""}`}>
      {#if selecting}<span class="text-[#5cb9ff]">{formatCount(Math.round(selectedTween.current))}</span> of {/if}{formatCount(Math.round(count.current))} item{targetCount === 1 ? "" : "s"}{#if selecting} selected{/if}{#if targetBytes > 0}<span
          class="px-1 text-[#5c5854]">·</span
        >{sizesPartial ? "≥ " : ""}{formatSize(bytes.current)}{/if}
    </span>
  {/if}
  {#if showFps}
    <span class={targetCount > 0 || activity || sizeScanning || sizeScanMessage ? "" : "ml-auto"}><FpsMeter /></span>
  {/if}
</footer>

<style>
  .size-scan-status { animation: scan-status-in 200ms ease-out both; }
  .size-scan-track { position: absolute; inset: -1px 0 auto; height: 1px; overflow: hidden; }
  .size-scan-track span {
    display: block;
    width: 28%;
    height: 100%;
    background: linear-gradient(90deg, transparent, #5cb9ff, transparent);
    animation: scan-sweep 1.8s ease-in-out infinite;
  }
  @keyframes scan-sweep {
    from { transform: translateX(-100%); }
    to { transform: translateX(460%); }
  }
  @keyframes scan-status-in {
    from { opacity: 0; transform: translateY(3px); }
    to { opacity: 1; transform: translateY(0); }
  }
  @media (prefers-reduced-motion: reduce) {
    .size-scan-status, .size-scan-track span { animation: none; }
    .size-scan-track span { width: 100%; opacity: 0.6; }
  }

  footer :global(.crumb) {
    padding: 0 4px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    font: inherit;
    line-height: 18px;
  }
  footer :global(button.crumb:hover),
  footer :global(button.crumb[data-state="open"]) {
    background: #353230;
    color: #e8e5e2;
  }
  footer :global(button.crumb:focus-visible) {
    outline: 1px solid #5cb9ff;
  }
  .crumb-current {
    color: #e8e5e2;
  }
  .crumb-separator {
    flex-shrink: 0;
    color: #5c5854;
  }

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
