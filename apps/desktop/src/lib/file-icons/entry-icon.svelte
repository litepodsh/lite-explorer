<script lang="ts">
  import type { Component } from "svelte";
  import ArchiveIcon from "@lucide/svelte/icons/file-archive";
  import CodeIcon from "@lucide/svelte/icons/file-code";
  import DocumentIcon from "@lucide/svelte/icons/file-text";
  import FileIcon from "@lucide/svelte/icons/file";
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
  }: { path: string; name: string; native?: boolean } = $props();

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
    default: { icon: FileIcon, class: "text-[#aaa5a1]" },
  };

  const source = $derived(native ? iconFor(path) : undefined);
  const fallback = $derived(FALLBACK_ICONS[categoryFor(name)]);

  $effect(() => {
    if (native) requestIcon(path);
  });
</script>

{#if source}
  <img
    src={source}
    alt=""
    width="17"
    height="17"
    class="size-[17px] shrink-0 object-contain"
    draggable="false" />
{:else}
  {@const Fallback = fallback.icon}
  <Fallback class="size-[17px] {fallback.class} stroke-[1.7]" />
{/if}
