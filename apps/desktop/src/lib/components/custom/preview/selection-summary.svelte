<script lang="ts">
  import FileIcon from "@lucide/svelte/icons/file";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import { Tween } from "svelte/motion";
  import { quartOut } from "svelte/easing";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
  import { formatSize } from "./format.js";

  let { entries }: { entries: DirectoryEntry[] } = $props();

  const folders = $derived(entries.filter((entry) => entry.is_directory).length);
  const files = $derived(entries.length - folders);
  const bytes = $derived(entries.reduce((sum, entry) => sum + (entry.size ?? 0), 0));
  // Folder sizes arrive later from the background scan; until then the total is a lower bound.
  const sizesPending = $derived(entries.some((entry) => entry.sizeComplete === false || (entry.is_directory && entry.size == null)));

  /** Most common kinds: extension for files, "Folder" for folders. */
  const kinds = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const entry of entries) {
      const dot = entry.name.lastIndexOf(".");
      const kind = entry.kind === "share" ? "Share" : entry.is_directory ? "Folder" : dot > 0 ? entry.name.slice(dot + 1).toUpperCase() : "File";
      counts.set(kind, (counts.get(kind) ?? 0) + 1);
    }
    return [...counts].sort((a, b) => b[1] - a[1]).slice(0, 4);
  });

  /** Up to three icons for the fanned stack, front first. */
  const stack = $derived(entries.slice(0, 3).map((entry) => (entry.kind === "share" ? "drive" : entry.is_directory ? "folder" : "file")));

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const count = new Tween(0, { duration: reduced ? 0 : 420, easing: quartOut });
  const size = new Tween(0, { duration: reduced ? 0 : 420, easing: quartOut });
  $effect(() => {
    count.target = entries.length;
    size.target = bytes;
  });

  const icons = { folder: FolderIcon, file: FileIcon, drive: HardDriveIcon };
</script>

<aside class="flex h-full min-w-0 flex-col bg-[#242220]" aria-label="Selection summary">
  <div class="summary-shell m-3 mb-0 flex min-h-0 flex-1 flex-col">
    <div class="summary-core flex min-h-0 flex-1 flex-col items-center justify-center gap-6 px-6">
      <div class="summary-stack" aria-hidden="true">
        {#each [...stack].reverse() as icon, depth (depth)}
          {@const Icon = icons[icon as keyof typeof icons]}
          {@const position = stack.length - 1 - depth}
          <span class="summary-card" data-position={position} data-icon={icon}><Icon /></span>
        {/each}
      </div>
      <div class="flex flex-col items-center gap-1 text-center" aria-live="polite">
        <p class="summary-count">{Math.round(count.current)}</p>
        <p class="text-[12px] tracking-[0.14em] text-[#9c9895] uppercase">items selected</p>
      </div>
    </div>
  </div>
  <section class="shrink-0 px-4 pt-3 pb-4" aria-label="Selection information">
    <h2 class="truncate text-[15px] font-semibold text-[#e8e5e2]">{entries.length} Items</h2>
    <p class="mt-0.5 text-[13px] text-[#9c9895]">
      {#if folders}{folders} {folders === 1 ? "folder" : "folders"}{/if}{#if folders && files} · {/if}{#if files}{files} {files === 1 ? "file" : "files"}{/if}
    </p>
    <h3 class="mt-4 mb-1.5 text-[13px] font-semibold text-[#e8e5e2]">Information</h3>
    <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[12px]">
      <dt class="text-[#9c9895]">Total size</dt>
      <dd class="m-0 text-right tabular-nums text-[#e8e5e2]">{sizesPending ? "≥ " : ""}{formatSize(Math.round(size.current))}</dd>
      <dt class="text-[#9c9895]">Kinds</dt>
      <dd class="m-0 flex flex-wrap justify-end gap-1">
        {#each kinds as [kind, n] (kind)}
          <span class="summary-chip">{kind}<span class="text-[#9c9895]">{n}</span></span>
        {/each}
      </dd>
    </dl>
  </section>
</aside>

<style>
  .summary-shell {
    --ease: cubic-bezier(0.32, 0.72, 0, 1);
    padding: 4px;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.025);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.05);
  }

  .summary-core {
    border-radius: 8px;
    background: radial-gradient(120% 80% at 50% 38%, rgb(10 132 255 / 0.07), transparent 60%), #1f1d1b;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.04);
    animation: summary-in 360ms var(--ease) both;
  }

  .summary-stack {
    position: relative;
    width: 76px;
    height: 88px;
  }

  .summary-card {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    border-radius: 14px;
    background: linear-gradient(180deg, #34312e, #2a2725);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.08),
      0 0 0 1px rgb(255 255 255 / 0.06),
      0 12px 28px -12px rgb(0 0 0 / 0.7);
    color: #aaa5a1;
    transition: transform 420ms var(--ease);
    animation: summary-card-in 420ms var(--ease) both;
  }

  .summary-card[data-icon="folder"],
  .summary-card[data-icon="drive"] {
    color: #0a9bff;
  }

  .summary-card :global(svg) {
    width: 30px;
    height: 30px;
    stroke-width: 1.4;
  }

  .summary-card[data-position="0"] {
    transform: translateY(0) rotate(0deg);
  }

  .summary-card[data-position="1"] {
    transform: translate(-16px, 5px) rotate(-9deg) scale(0.94);
    opacity: 0.85;
    animation-delay: 40ms;
  }

  .summary-card[data-position="2"] {
    transform: translate(16px, 5px) rotate(9deg) scale(0.94);
    opacity: 0.7;
    animation-delay: 80ms;
  }

  .summary-count {
    margin: 0;
    color: #eceae8;
    font-size: 30px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    letter-spacing: -0.02em;
  }

  .summary-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 7px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.05);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.06);
    color: #e8e5e2;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  @keyframes summary-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }

  @keyframes summary-card-in {
    from {
      opacity: 0;
      transform: translateY(10px) rotate(0deg) scale(0.9);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .summary-core,
    .summary-card {
      animation: none;
      transition: none;
    }
  }
</style>
