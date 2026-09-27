<script lang="ts">
  import type { Component } from "svelte";
  import ArchiveIcon from "@lucide/svelte/icons/file-archive";
  import CodeIcon from "@lucide/svelte/icons/file-code";
  import DocumentIcon from "@lucide/svelte/icons/file-text";
  import FontIcon from "@lucide/svelte/icons/type";
  import ImageIcon from "@lucide/svelte/icons/file-image";
  import MusicIcon from "@lucide/svelte/icons/file-music";
  import PdfIcon from "@lucide/svelte/icons/file-type";
  import PresentationIcon from "@lucide/svelte/icons/presentation";
  import SheetIcon from "@lucide/svelte/icons/file-spreadsheet";
  import TypeIcon from "@lucide/svelte/icons/file-text";
  import VideoIcon from "@lucide/svelte/icons/file-video-camera";
  import { categoryFor, type IconCategory } from "./fallback.js";
  import { iconFor, requestIcon } from "./icon-cache.svelte.js";

  let {
    path,
    name,
    native = false,
    size = 17,
  }: { path: string; name: string; native?: boolean; size?: number } = $props();

  const FALLBACK_ICONS: Record<IconCategory, { icon: Component; class: string }> = {
    archive: { icon: ArchiveIcon, class: "text-[#c9a06a]" },
    image: { icon: ImageIcon, class: "text-[#7fb37f]" },
    video: { icon: VideoIcon, class: "text-[#b48ad0]" },
    audio: { icon: MusicIcon, class: "text-[#d08a9f]" },
    code: { icon: CodeIcon, class: "text-[#7fa8d0]" },
    text: { icon: TypeIcon, class: "text-[#9c9895]" },
    pdf: { icon: PdfIcon, class: "text-[#d08a7f]" },
    document: { icon: DocumentIcon, class: "text-[#8fa3c0]" },
    sheet: { icon: SheetIcon, class: "text-[#7fb3a0]" },
    slides: { icon: PresentationIcon, class: "text-[#c9a06a]" },
    font: { icon: FontIcon, class: "text-[#b0a0c0]" },
    default: { icon: DocumentIcon, class: "text-[#aaa5a1]" },
  };

  const category = $derived(categoryFor(name));
  const source = $derived(native && category !== "default" ? iconFor(path) : undefined);
  const fallback = $derived(FALLBACK_ICONS[category]);

  $effect(() => {
    if (native && category !== "default") requestIcon(path);
  });
</script>

{#if category === "default"}
  <img class="file-light-icon size-[17px] shrink-0 object-contain" style={`width: ${size}px; height: ${size}px`} src="/icons/file_light.svg" alt="" draggable="false" />
  <img class="file-dark-icon size-[17px] shrink-0 object-contain" style={`width: ${size}px; height: ${size}px`} src="/icons/file_dark.svg" alt="" draggable="false" />
{:else if source}
  <img
    src={source}
    alt=""
    width={size}
    height={size}
    style={`width: ${size}px; height: ${size}px`}
    class="size-[17px] shrink-0 object-contain"
    draggable="false" />
{:else}
  {@const Fallback = fallback.icon}
  <Fallback class="size-[17px] {fallback.class} stroke-[1.7]" style={`width: ${size}px; height: ${size}px`} />
{/if}

<style>
  .file-light-icon { display: none; }
  :global(:root[data-theme="light"] .file-light-icon),
  :global(:root[data-theme="off-white"] .file-light-icon) { display: block; }
  :global(:root[data-theme="light"] .file-dark-icon),
  :global(:root[data-theme="off-white"] .file-dark-icon) { display: none; }
</style>
