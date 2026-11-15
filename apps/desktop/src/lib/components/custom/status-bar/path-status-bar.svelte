<script lang="ts">
  import GitBranchIcon from "@lucide/svelte/icons/git-branch";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import { Tween, prefersReducedMotion } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import { formatCount, formatSize } from "#lib/components/custom/preview/format.js";
  import type { DirectoryEntry } from "#lib/components/custom/file-list/list-item.svelte";
  import FpsMeter from "./fps-meter.svelte";
  import { diskSpace } from "#lib/system/disk-space.svelte.js";
  import { gitBranch } from "#lib/system/git-branch.js";

  let { location = "", entries = [], selectedEntries = [], showFps = false, activity = null, keyboardMode = "standard", visual = false, pendingKeys = "", sizeScanning = false, sizeScanMessage = "", onCancelSizeScan } = $props<{
    sizeScanning?: boolean;
    sizeScanMessage?: string;
    onCancelSizeScan?: () => void;
    /** The internal path of the listing; the branch and free space describe it. The path itself is in the toolbar. */
    location?: string;
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

  let branch = $state<string | null>(null);
  $effect(() => {
    const path = location;
    let current = true;
    void gitBranch(path).then((name) => {
      if (current) branch = name;
    });
    return () => (current = false);
  });

  // Free space is read again when the folder changes and the last reading is stale.
  $effect(() => {
    if (location) void diskSpace.refresh();
  });
  const volume = $derived(diskSpace.volumeFor(location));
</script>

<footer
  class="status-bar relative flex h-7 shrink-0 items-center gap-3 border-t border-[var(--app-border)] bg-[var(--app-bg)] px-3 text-[11px] text-[var(--app-fg-muted)]"
  aria-label="Folder status">
  {#if sizeScanning}<span class="size-scan-track" aria-hidden="true"><span></span></span>{/if}
  {#if keyboardMode === "yazi" || visual || pendingKeys}
    <span class="inline-flex shrink-0 items-center gap-1">
      {#if keyboardMode === "yazi"}<span class="key-pill">YAZI</span>{/if}
      {#if visual}<span class="key-pill key-pill-visual">VISUAL</span>{/if}
      {#if pendingKeys}<span class="key-pill key-pill-pending" aria-live="polite">{pendingKeys} …</span>{/if}
    </span>
  {/if}
  {#if branch}
    <span class="status-item min-w-0" title={`Git branch ${branch}`}><GitBranchIcon /><span class="truncate">{branch}</span></span>
  {/if}
  {#if sizeScanning || sizeScanMessage}
    <span class="size-scan-status ml-auto inline-flex min-w-0 items-center gap-1.5" role="status">
      {#if sizeScanning}
        <LoaderCircleIcon class="size-3 shrink-0 animate-spin text-[var(--app-accent)] motion-reduce:animate-none" />
        <span class="truncate">Calculating sizes…</span>
      {:else}
        <span class="truncate" title={sizeScanMessage}>{sizeScanMessage}</span>
      {/if}
    </span>
    {#if sizeScanning}
      <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[var(--app-fg-muted)] transition-colors hover:bg-[var(--app-border-strong)] hover:text-[var(--app-fg)] focus-visible:outline focus-visible:outline-1 focus-visible:outline-[var(--app-accent)]" onclick={onCancelSizeScan}>Cancel</button>
    {/if}
  {:else if activity}
    <span class="ml-auto inline-flex shrink-0 items-center gap-1.5 tabular-nums text-[var(--app-fg-muted)]" aria-live="polite">
      <LoaderCircleIcon class="size-3 animate-spin text-[var(--app-accent)] motion-reduce:animate-none" />{activity}
    </span>
  {/if}
  {#if volume}
    <span
      class="status-item {activity || sizeScanning || sizeScanMessage ? '' : 'ml-auto'}"
      title={`${volume.name}: ${formatSize(volume.free_bytes)} free of ${formatSize(volume.total_bytes)}`}><HardDriveIcon />{formatSize(volume.free_bytes)} free</span>
  {/if}
  {#if targetCount > 0}
    <span
      class="{activity || sizeScanning || sizeScanMessage || volume ? '' : 'ml-auto'} shrink-0 tabular-nums text-[var(--app-fg-muted)]"
      aria-live="polite"
      aria-label={`${selecting ? `${formatCount(selectedEntries.length)} of ` : ""}${formatCount(targetCount)} items${selecting ? " selected" : ""}${targetBytes > 0 ? `, ${sizesPartial ? "at least " : ""}${formatSize(targetBytes)}` : ""}`}>
      {#if selecting}<span class="text-[var(--app-accent)]">{formatCount(Math.round(selectedTween.current))}</span>{" of "}{/if}{formatCount(Math.round(count.current))} item{targetCount === 1 ? "" : "s"}{#if selecting}{" selected"}{/if}{#if targetBytes > 0}<span
          class="px-1 text-[var(--app-fg-faint)]">·</span
        >{sizesPartial ? "≥ " : ""}{formatSize(bytes.current)}{/if}
    </span>
  {/if}
  {#if showFps}
    <span class={targetCount > 0 || volume || activity || sizeScanning || sizeScanMessage ? "" : "ml-auto"}><FpsMeter /></span>
  {/if}
</footer>

<style>
  .size-scan-status { animation: scan-status-in 200ms ease-out both; }
  .size-scan-track { position: absolute; inset: -1px 0 auto; height: 1px; overflow: hidden; }
  .size-scan-track span {
    display: block;
    width: 28%;
    height: 100%;
    background: linear-gradient(90deg, transparent, var(--app-accent), transparent);
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

  .status-bar {
    font-family: var(--app-font-mono);
    font-size: 10.5px;
  }
  .status-item {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .status-item :global(svg) {
    flex: none;
    width: 12px;
    height: 12px;
    stroke-width: 1.8;
  }

  .key-pill {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--app-hover);
    color: var(--app-fg-muted);
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    line-height: 16px;
  }
  .key-pill-visual {
    background: color-mix(in srgb, var(--app-accent) 18%, transparent);
    color: var(--app-accent);
  }
  .key-pill-pending {
    background: rgb(255 184 76 / 16%);
    color: var(--app-warning);
    letter-spacing: 0.02em;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  }
</style>
