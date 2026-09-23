<script lang="ts">
  import { settings } from "$lib/settings/settings.svelte.js";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import FileIcon from "@lucide/svelte/icons/file";
  import ExternalLinkIcon from "@lucide/svelte/icons/external-link";
  import WindowControls from "$lib/components/custom/titlebar/window-controls.svelte";
  import ImageView from "$lib/components/custom/preview/image-view.svelte";
  import MediaView from "$lib/components/custom/preview/media-view.svelte";
  import PdfView from "$lib/components/custom/preview/pdf-view.svelte";
  import { fetchMediaUrl, fetchViewerTarget } from "$lib/components/custom/preview/media.js";
  import { isMediaKind } from "$lib/components/custom/preview/languages.js";
  import type { FilePreview } from "$lib/components/custom/preview/types.js";
  import type { Platform } from "$lib/state/platform.svelte.js";
  import { openTarget } from "$lib/file-ops/open.js";

  settings.load();

  let preview = $state.raw<FilePreview | null>(null);
  let mediaUrl = $state("");
  let path = $state("");
  let error = $state("");
  let platform = $state<Exclude<Platform, "unknown">>("windows");
  const controlsPlatform = $derived(platform === "macos" ? null : platform);

  const appWindow = getCurrentWindow();

  async function load(next: string) {
    if (!next) return;
    path = next;
    preview = null;
    mediaUrl = "";
    error = "";
    try {
      const result = await invoke<FilePreview>("read_file_preview", { path: next });
      preview = result;
      if (isMediaKind(result.kind)) {
        mediaUrl = (await fetchMediaUrl(next)).url;
      }
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    }
  }

  async function close() {
    if (await appWindow.isFullscreen().catch(() => false)) {
      await appWindow.setFullscreen(false).catch(() => {});
      return;
    }
    void appWindow.close();
  }

  function handleKeydown(event: KeyboardEvent) {
    // Cmd/Ctrl+W closes this window. macOS also routes it through the app menu;
    // this covers the webview path on Windows/Linux.
    if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === "w") {
      event.preventDefault();
      void appWindow.close();
      return;
    }
    if (event.key !== "Escape") return;
    // While a player element is fullscreen, Escape exits fullscreen (browser default)
    // instead of closing the window.
    const doc = document as Document & { webkitFullscreenElement?: Element | null };
    if (document.fullscreenElement ?? doc.webkitFullscreenElement) return;
    event.preventDefault();
    void close();
  }

  onMount(() => {
    void invoke<string>("os_detection").then((detected) => {
      if (detected === "macos" || detected === "windows" || detected === "linux") {
        platform = detected;
      }
    });
    void fetchViewerTarget().then((target) => {
      if (target) void load(target);
    });
    const stopSettings = settings.listen();
    const unlisten = listen<{ path: string }>("viewer-open", (event) => void load(event.payload.path));
    return () => {
      stopSettings();
      void unlisten.then((stop) => stop());
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="viewer-window flex h-screen w-screen flex-col overflow-hidden bg-[#141211] text-[#e8e5e2]"
  data-platform={platform}
>
  <header
    class="viewer-titlebar relative flex h-11 shrink-0 items-center justify-center px-4"
    data-tauri-drag-region
  >
    <span class="pointer-events-none max-w-[60%] truncate text-[12.5px] font-medium text-[#9c9895]" data-tauri-drag-region>
      {preview?.name ?? ""}
    </span>
    {#if controlsPlatform}
      <div class="absolute top-0 right-0 h-full">
        <WindowControls platform={controlsPlatform} />
      </div>
    {/if}
  </header>

  <div class="relative min-h-0 flex-1">
    {#if error}
      <div class="grid h-full place-content-center justify-items-center gap-2 px-6 text-center">
        <FileIcon class="mb-1 size-12 stroke-[1.2] text-[#67635f]" />
        <p class="text-[14px] font-semibold text-[#e8e5e2]">Can’t preview this file</p>
        <p class="text-[12px] break-all text-[#9c9895]">{error}</p>
        {#if path}
          <button
            type="button"
            class="mt-1 flex items-center gap-1.5 rounded-full border-0 bg-white/[0.06] px-3.5 py-1.5 text-[12px] font-medium text-[#e8e5e2] transition-[background-color,transform] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/[0.12] active:scale-[0.97]"
            onclick={() => openTarget(path)}
          >
            <ExternalLinkIcon class="size-3.5 stroke-[1.8]" /> Open with default app
          </button>
        {/if}
      </div>
    {:else if preview && isMediaKind(preview.kind)}
      {#if mediaUrl}
        {#if preview.kind === "image"}
          <ImageView src={mediaUrl} name={preview.name} />
        {:else if preview.kind === "pdf"}
          <PdfView src={mediaUrl} name={preview.name} />
        {:else if preview.kind === "video" || preview.kind === "audio"}
          <MediaView src={mediaUrl} name={preview.name} kind={preview.kind} path={path} autofocus />
        {/if}
      {:else}
        <div class="grid h-full place-items-center">
          <span class="size-7 animate-spin rounded-full border-2 border-white/20 border-t-white/70" aria-label="Loading"></span>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .viewer-window[data-platform="macos"] .viewer-titlebar {
    padding-left: 84px;
  }
</style>
