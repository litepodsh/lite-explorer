<script lang="ts">
  import CpuIcon from "@lucide/svelte/icons/cpu";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import LaptopIcon from "@lucide/svelte/icons/laptop";
  import MemoryStickIcon from "@lucide/svelte/icons/memory-stick";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import { UsageBar } from "$lib/components/custom/usage-bar/index.js";
  import type { DeviceInfo, VolumeInfo } from "./types.js";
  import { diskBreakdown } from "./usage.js";

  let {
    device,
    volumes,
    homeBytes,
  }: {
    device: DeviceInfo;
    volumes: VolumeInfo[];
    /** Size of the home folder, used to split the startup volume. Null until measured. */
    homeBytes: number | null;
  } = $props();

  const HOME_COLOR = "#0a9bff";
  const SYSTEM_COLOR = "#7b7773";
  const FREE_COLOR = "#3a3734";

  // Segment the pointer is over (or a legend row is emphasizing). Drives the
  // active bar segment and dims the other legend entries while set.
  let activeSegment = $state<string | null>(null);

  function legendClass(id: string) {
    if (!activeSegment) return "";
    return activeSegment === id ? "text-[var(--app-fg)]" : "opacity-40";
  }

  // Memory is sold in binary gigabytes, so 25769803776 bytes reads as "24 GB".
  const memory = $derived(
    device.memory_bytes ? `${Math.round(device.memory_bytes / 1024 ** 3)} GB` : null,
  );

  function segmentsFor(volume: VolumeInfo) {
    const split = diskBreakdown(volume, volume.is_primary ? homeBytes : null);
    if (volume.is_primary && homeBytes !== null) {
      return [
        { id: "home", value: split.home, color: HOME_COLOR, label: `Your home folder: ${formatSize(split.home)}` },
        { id: "system", value: split.system, color: SYSTEM_COLOR, label: `Apps & system: ${formatSize(split.system)}` },
      ];
    }
    return [{ id: "used", value: split.used, color: HOME_COLOR, label: `Used: ${formatSize(split.used)}` }];
  }
</script>

<article class="overflow-hidden rounded-[var(--app-radius)] border border-[var(--app-border)] bg-[var(--app-surface)]">
  <header class="flex items-center gap-3 border-b border-[var(--app-border)] px-4 py-3.5">
    <LaptopIcon class="size-7 shrink-0 stroke-[1.4] text-blue-400" />
    <div class="min-w-0">
      <h2 class="flex flex-wrap items-baseline gap-x-2 text-[15px] font-semibold text-[var(--app-fg)]">
        <span class="truncate">{device.name}</span>
        <span class="text-xs font-normal text-[var(--app-fg-muted)]">This device</span>
      </h2>
      <p class="mt-0.5 text-xs text-[var(--app-fg-muted)]">{volumes.length} volume{volumes.length === 1 ? "" : "s"}</p>
    </div>
    <div class="ml-auto grid shrink-0 justify-items-end gap-1 text-right">
      {#if device.chip}<strong class="text-[13px] font-medium text-[var(--app-fg)]">{device.chip}</strong>{/if}
      <div class="flex gap-3 text-xs tabular-nums text-[var(--app-fg-muted)]">
        <span class="inline-flex items-center gap-1" title="CPU cores"><CpuIcon class="size-3.5" />{device.cores}</span>
        {#if memory}
          <span class="inline-flex items-center gap-1" title="Memory"><MemoryStickIcon class="size-3.5" />{memory}</span>
        {/if}
      </div>
    </div>
  </header>

  <div class="grid gap-3 p-3">
    {#each volumes as volume (volume.mount_point)}
      {@const split = diskBreakdown(volume, volume.is_primary ? homeBytes : null)}
      <section class="grid gap-3.5 rounded-[calc(var(--app-radius)-3px)] border border-[var(--app-border)] bg-[var(--app-input)] p-3.5">
        <div class="flex items-center gap-3">
          <HardDriveIcon class="size-7 shrink-0 stroke-[1.4] text-[var(--app-fg-muted)]" />
          <div class="min-w-0">
            <p class="truncate text-[13px] font-medium text-[var(--app-fg)]" title={volume.mount_point}>{volume.name}</p>
            <div class="mt-1 flex gap-1.5">
              {#if volume.file_system}
                <span class="rounded-[5px] border border-[var(--app-border)] px-1.5 text-[11px] text-[var(--app-fg-muted)]">{volume.file_system}</span>
              {/if}
              <span class="rounded-[5px] border border-[var(--app-border)] px-1.5 text-[11px] text-[var(--app-fg-muted)]"
                >{volume.is_primary ? "Primary" : "External"}</span>
            </div>
          </div>
          <div class="ml-auto shrink-0 text-right tabular-nums">
            <p class="text-[13px] font-medium text-[var(--app-fg)]">{formatSize(volume.total_bytes)}</p>
            <p class="text-xs text-[var(--app-fg-muted)]">{formatSize(split.free)} free</p>
          </div>
        </div>
        <UsageBar
          segments={segmentsFor(volume)}
          total={volume.total_bytes}
          activeId={activeSegment ?? undefined}
          onHover={(id) => (activeSegment = id)}
          label={`${volume.name}: ${formatSize(split.used)} used of ${formatSize(volume.total_bytes)}`} />
        <div class="flex flex-wrap gap-x-4 gap-y-1.5 text-xs tabular-nums text-[var(--app-fg-muted)]">
          {#if volume.is_primary && homeBytes !== null}
            <span class="inline-flex items-center gap-1.5 {legendClass('home')}"
              ><i class="size-2 rounded-full" style:background={HOME_COLOR}></i>Your home folder
              <b class="font-medium text-[var(--app-fg)]">{formatSize(split.home)}</b></span>
            <span class="inline-flex items-center gap-1.5 {legendClass('system')}"
              ><i class="size-2 rounded-full" style:background={SYSTEM_COLOR}></i>Apps & system
              <b class="font-medium text-[var(--app-fg)]">{formatSize(split.system)}</b></span>
          {:else}
            <span class="inline-flex items-center gap-1.5 {legendClass('used')}"
              ><i class="size-2 rounded-full" style:background={HOME_COLOR}></i>Used
              <b class="font-medium text-[var(--app-fg)]">{formatSize(split.used)}</b></span>
          {/if}
          <span class="inline-flex items-center gap-1.5 {legendClass('free')}"
            ><i class="size-2 rounded-full" style:background={FREE_COLOR}></i>Free
            <b class="font-medium text-[var(--app-fg)]">{formatSize(split.free)}</b></span>
        </div>
      </section>
    {:else}
      <p class="px-1 py-2 text-xs text-[var(--app-fg-muted)]">No volumes found.</p>
    {/each}
  </div>
</article>
