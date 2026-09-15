<script lang="ts">
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import { downloadFraction, isPartialDownload, type DownloadSnapshot } from "./download-progress.js";
  import { fileDownloads } from "./download-progress.svelte.js";

  let { path, name, snapshot }: { path: string; name: string; snapshot?: DownloadSnapshot } = $props();
  const transfer = $derived(Object.values(fileDownloads.jobs).find((file) => file.path === path));
  const fraction = $derived(transfer
    ? downloadFraction(transfer.bytesDone, transfer.bytesTotal)
    : snapshot?.fraction ?? null);
  const bytes = $derived(transfer?.bytesDone ?? snapshot?.bytesDone);
  const visible = $derived(!!transfer || (!snapshot?.missing &&
    (fraction !== null ? fraction < 1 : isPartialDownload(name))));
  const percent = $derived(fraction === null ? null : Math.floor(fraction * 1000) / 10);
  const label = $derived(fraction !== null
    ? `${percent}% downloaded${bytes != null ? ` · ${formatSize(bytes)}` : ""}${transfer && transfer.bytesTotal > 0 ? ` of ${formatSize(transfer.bytesTotal)}` : ""}`
    : transfer ? `Downloading${bytes != null ? ` · ${formatSize(bytes)}` : ""}` : "Incomplete download");
</script>

{#if visible}
  <Tooltip.Provider delayDuration={300}>
  <Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
  <!-- Focus exposes the tooltip without nesting a button inside the grid tile. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <span
    {...props}
    tabindex="0"
    class="download-indicator"
    class:indeterminate={!!transfer && fraction === null}
    class:incomplete={!transfer && fraction === null}
    role="progressbar"
    aria-label={`${name}: ${label}`}
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={percent ?? undefined}
    aria-valuetext={label}>
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <circle class="track" cx="10" cy="10" r="7.5" />
      {#if fraction !== null || transfer}
        <circle class="fill" cx="10" cy="10" r="7.5" pathLength="100"
          stroke-dasharray={`${fraction === null ? 25 : fraction * 100} 100`} />
      {:else}
        <path class="mark" d="M10 6v5m0 3h.01" />
      {/if}
    </svg>
  </span>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content side="top" sideOffset={8}
    class="block min-w-44 rounded-xl bg-[#302d2a] p-1 text-[#e8e5e2] ring-1 ring-white/10 shadow-[0_8px_32px_rgb(0_0_0/0.18)] duration-150 ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:animate-none"
    arrowClasses="bg-[#302d2a]">
    <div class="rounded-lg bg-[#242220] px-3 py-2.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.04)]">
      <div class="flex items-baseline justify-between gap-5">
        <span class="text-[11px] text-[#b4aeaa]">{fraction !== null ? "Downloaded" : transfer ? "Downloading" : "Incomplete download"}</span>
        {#if percent !== null}<span class="text-lg leading-none font-medium tracking-tight text-[#72c3ff] tabular-nums">{percent}<span class="ml-0.5 text-xs text-[#b4aeaa]">%</span></span>{/if}
      </div>
      {#if bytes != null}
        <p class="mt-1.5 text-[11px] text-[#9c9895] tabular-nums">{formatSize(bytes)}{#if transfer && transfer.bytesTotal > 0} of {formatSize(transfer.bytesTotal)}{:else} downloaded{/if}</p>
      {/if}
      {#if fraction === null}<p class="mt-1 text-[11px] text-[#9c9895]">{transfer ? "Total size unavailable" : "No live progress available"}</p>{/if}
    </div>
  </Tooltip.Content>
  </Tooltip.Root>
  </Tooltip.Provider>
{/if}

<style>
  .download-indicator { display: inline-flex; width: 16px; height: 16px; flex: none; color: #52b5ff; }
  .download-indicator:focus-visible { outline: 2px solid #52b5ff; outline-offset: 3px; border-radius: 50%; }
  svg { width: 100%; height: 100%; overflow: visible; fill: none; stroke: currentColor; stroke-width: 1.6; }
  .track { opacity: 0.22; }
  .fill { transform: rotate(-90deg); transform-origin: center; stroke-linecap: round; }
  .mark { stroke-linecap: round; }
  .incomplete { color: #9c9895; }
  .indeterminate svg { animation: download-turn 1.4s cubic-bezier(0.32, 0.72, 0, 1) infinite; }
  @keyframes download-turn { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .indeterminate svg { animation: none; } }
</style>
