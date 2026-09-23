<script lang="ts">
  import CircleDashedIcon from "@lucide/svelte/icons/circle-dashed";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import HouseIcon from "@lucide/svelte/icons/house";
  import LayersIcon from "@lucide/svelte/icons/layers";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import DeviceCard from "./device-card.svelte";
  import { folderScan } from "./folder-scan.svelte.js";
  import FolderUsageCard from "./folder-usage-card.svelte";
  import TrashCard from "./trash-card.svelte";
  import type { DiskOverview, FolderUsageEntry } from "./types.js";
  import { diskBreakdown } from "./usage.js";

  let {
    showHidden = false,
    onOpen,
  }: {
    showHidden?: boolean;
    onOpen?: (entry: FolderUsageEntry) => void;
  } = $props();

  let overview = $state<DiskOverview | null>(null);
  let loadError = $state("");
  let nowSeconds = $state(Date.now() / 1000);
  let os = $state<string | null>(null);

  const primary = $derived(overview?.volumes.find((volume) => volume.is_primary) ?? overview?.volumes[0] ?? null);
  // While the very first scan runs, the running byte count is the best home size we have.
  const homeBytes = $derived(
    folderScan.usage?.scanned_at != null
      ? folderScan.usage.total_bytes
      : folderScan.scanning && folderScan.scannedBytes > 0
        ? folderScan.scannedBytes
        : null,
  );
  const split = $derived(primary ? diskBreakdown(primary, homeBytes) : null);
  const fileSystems = $derived([
    ...new Set(overview?.volumes.map((volume) => volume.file_system).filter(Boolean) ?? []),
  ]);

  onMount(() => {
    const clock = setInterval(() => (nowSeconds = Date.now() / 1000), 30_000);
    void readDisks();
    void invoke<string>("os_detection").then((value) => (os = value)).catch(() => {});
    void folderScan.load().catch((reason) => (loadError = String(reason)));
    return () => clearInterval(clock);
  });

  // A long walk can outlive changes to free space, so read the volumes again when it ends.
  let seenCompletions = folderScan.completions;
  $effect(() => {
    if (folderScan.completions === seenCompletions) return;
    seenCompletions = folderScan.completions;
    void readDisks();
  });

  async function readDisks() {
    try {
      overview = await invoke<DiskOverview>("disk_overview");
      nowSeconds = Date.now() / 1000;
    } catch (reason) {
      loadError = reason instanceof Error ? reason.message : String(reason);
    }
  }

  function share(part: number, whole: number) {
    return whole > 0 ? `${Math.round((part / whole) * 100)}%` : "0%";
  }

  function sizeParts(bytes: number) {
    const [value, unit = ""] = formatSize(bytes).split(" ");
    return { value, unit };
  }
</script>

{#snippet stat(Icon: typeof HardDriveIcon, value: string, unit: string, label: string, sub: string, highlight = "")}
  <div class="grid grid-cols-[34px_minmax(0,1fr)] gap-x-3 px-1 py-1.5">
    <Icon class="mt-1.5 size-[26px] stroke-[1.5] {highlight ? 'text-[var(--app-accent)]' : 'text-[var(--app-fg-muted)]'}" />
    <div class="min-w-0">
      <p class="text-[30px] leading-[1.1] font-semibold tracking-[-0.02em] tabular-nums text-[var(--app-fg)]">
        {value}{#if unit}<small class="ml-1 text-[15px] font-medium tracking-normal text-[var(--app-fg-muted)]">{unit}</small>{/if}
      </p>
      <p class="mt-1.5 text-[13px] text-[var(--app-fg)]">{label}</p>
      <p class="mt-0.5 truncate text-xs tabular-nums text-[var(--app-fg-muted)]">
        {#if highlight}<b class="font-medium text-[var(--app-accent)]">{highlight}</b>{/if} {sub}
      </p>
    </div>
  </div>
{/snippet}

<section class="overview-panel @container min-h-0 flex-1 overflow-auto" aria-label="Overview">
  <div class="grid content-start gap-6 px-7 pt-6 pb-8">
    {#if loadError}
      <p class="text-[13px] text-[#ff8a80]">Couldn’t read your disks: {loadError}</p>
    {:else if !overview || !primary || !split}
      <div class="grid grid-cols-2 gap-2 @3xl:grid-cols-4" aria-hidden="true">
        {#each [0, 1, 2, 3] as item (item)}
          <div class="grid gap-2 px-1 py-1.5 pl-[46px]">
            <span class="h-8 w-28 animate-pulse rounded bg-[var(--app-surface)] motion-reduce:animate-none"></span>
            <span class="h-3 w-20 rounded bg-[var(--app-surface)]"></span>
          </div>
        {/each}
      </div>
    {:else}
      {@const capacity = sizeParts(primary.total_bytes)}
      {@const free = sizeParts(split.free)}
      {@const home = homeBytes === null ? null : sizeParts(homeBytes)}
      <div class="grid grid-cols-2 gap-2 @3xl:grid-cols-4">
        {@render stat(
          HardDriveIcon,
          capacity.value,
          capacity.unit,
          "Total capacity",
          "used",
          formatSize(split.used),
        )}
        {@render stat(CircleDashedIcon, free.value, free.unit, "Free space", `${share(split.free, primary.total_bytes)} of the disk`)}
        {@render stat(
          HouseIcon,
          home?.value ?? "…",
          home?.unit ?? "",
          "Your home folder",
          home ? `${share(split.home, split.used)} of used space` : "Measuring…",
        )}
        {@render stat(
          LayersIcon,
          String(overview.volumes.length),
          "",
          overview.volumes.length === 1 ? "Volume" : "Volumes",
          fileSystems.join(", ") || "Mounted",
        )}
      </div>

      <div class="grid grid-cols-1 items-start gap-4 @4xl:grid-cols-[1.08fr_1fr]">
        <div class="grid content-start gap-4">
          <DeviceCard device={overview.device} volumes={overview.volumes} {homeBytes} />
          <TrashCard
            trashBytes={folderScan.usage?.trash_bytes ?? null}
            scanning={folderScan.scanning}
            {os}
            onEmptied={(bytes) => {
              if (folderScan.usage) folderScan.usage = { ...folderScan.usage, trash_bytes: bytes };
            }}
            onDetected={(bytes) => {
              if (folderScan.usage) folderScan.usage = { ...folderScan.usage, trash_bytes: bytes };
            }} />
        </div>
        <FolderUsageCard
          usage={folderScan.usage}
          scanning={folderScan.scanning}
          scannedBytes={folderScan.scannedBytes}
          currentFolder={folderScan.currentFolder}
          error={folderScan.error}
          {showHidden}
          {nowSeconds}
          onRefresh={() => void folderScan.refresh()}
          onOpen={(entry) => onOpen?.(entry)} />
      </div>
    {/if}
  </div>
</section>
