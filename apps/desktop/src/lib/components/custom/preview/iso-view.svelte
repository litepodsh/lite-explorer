<script lang="ts">
  import Disc from "@lucide/svelte/icons/disc";
  import { PathTree } from "$lib/components/custom/path-tree/index.js";
  import { formatSize } from "./format.js";
  import { openIso, type IsoPreview } from "./iso.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let iso = $state.raw<IsoPreview | null>(null);
  let selected = $state<string[]>([]);
  let error = $state("");
  let token = 0;

  // Directories end with "/", matching the tree's path convention.
  const treePaths = $derived(
    iso ? iso.entries.map((entry) => (entry.isDir ? `${entry.path}/` : entry.path)) : [],
  );
  const sizes = $derived(
    new Map(
      (iso?.entries ?? [])
        .filter((entry) => !entry.isDir)
        .map((entry) => [entry.path, entry.size] as const),
    ),
  );

  function decorate(treePath: string): string | null {
    const size = sizes.get(treePath);
    return size === undefined ? null : formatSize(size);
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    iso = null;
    selected = [];
    error = "";
    openIso(target)
      .then((result) => {
        if (request === token) iso = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="flex h-full min-h-0 flex-col bg-[#1f1d1b]" aria-label={`Disc image preview of ${name}`}>
  {#if error}
    <div class="grid flex-1 place-content-center justify-items-center gap-1 px-4 text-center">
      <Disc class="mb-2 size-12 stroke-[1.2] text-[#67635f]" />
      <p class="text-[14px] font-semibold text-[#e8e5e2]">Can’t preview this disc image</p>
      <p class="text-[12px] break-all text-[#9c9895]">{error}</p>
    </div>
  {:else if iso}
    <header class="shrink-0 px-3 pt-3 pb-2">
      <p class="truncate text-[13px] font-semibold text-[#e8e5e2]" title={name}>
        {iso.volumeName || name}
      </p>
      <p class="text-[12px] text-[#9c9895] tabular-nums">
        {iso.entries.length.toLocaleString("en-US")} entradas{iso.truncated ? " (mostrando las primeras 5000)" : ""}
      </p>
    </header>
    <div class="min-h-0 flex-1">
      <PathTree paths={treePaths} bind:selected {decorate} search initialExpansion={1} />
    </div>
  {/if}
</div>
