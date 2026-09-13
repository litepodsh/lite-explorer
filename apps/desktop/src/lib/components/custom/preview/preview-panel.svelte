<script lang="ts" module>
  // Remembered for the rest of the session, across panel mounts.
  let sessionViewMode: "render" | "code" = "render";
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import FileIcon from "@lucide/svelte/icons/file";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";
  import CodeView from "./code-view.svelte";
  import FileInfo from "./file-info.svelte";
  import FindBar from "./find-bar.svelte";
  import MarkdownView from "./markdown-view.svelte";
  import HtmlView from "./html-view.svelte";
  import ImageView from "./image-view.svelte";
  import BinaryView from "./binary-view.svelte";
  import { fontSizeForShortcut, lineHeightFor, parseFontSize } from "./font-size.js";
  import { isHtmlName, isMarkdownName, languageFor } from "./languages.js";
  import { analyzeHtmlSafety } from "./html-safety.js";
  import type { FilePreview } from "./types.js";
  import { fetchDefaultApp, type OpenWithApp } from "$lib/file-ops/open.js";
  import { isRemotePath } from "$lib/remote/remote-locations.js";

  type Props = { entry: DirectoryEntry };
  let { entry }: Props = $props();

  const DEBOUNCE_MS = 40;
  const SLOW_MS = 150;
  const FONT_SIZE_KEY = "preview-font-size";
  const MIN_HTML_ZOOM = 0.5;
  const MAX_HTML_ZOOM = 2;

  let preview = $state.raw<FilePreview | null>(null);
  let defaultApp = $state.raw<OpenWithApp | null>(null);
  let previewPath = $state("");
  let error = $state("");
  let slow = $state(false);
  let findOpen = $state(false);
  let viewMode = $state(sessionViewMode);
  let markdownFailed = $state(false);
  let fontSize = $state(parseFontSize(localStorage.getItem(FONT_SIZE_KEY)));
  let htmlZoom = $state(1);
  let contentRoot = $state<HTMLElement | null>(null);
  let codeView = $state<ReturnType<typeof CodeView> | null>(null);
  let findBar = $state<ReturnType<typeof FindBar> | null>(null);
  let token = 0;

  // Content may show the previous file while a request is pending; file
  // information hides stale values but keeps their space to avoid a jump.
  const pending = $derived(previewPath !== entry.path);
  const previewIsMarkdown = $derived(preview ? isMarkdownName(preview.name) : false);
  const previewIsHtml = $derived(preview ? isHtmlName(preview.name) : false);
  const showViewToggle = $derived(previewIsMarkdown || previewIsHtml);
  const showRendered = $derived(
    (previewIsMarkdown && viewMode === "render" && !markdownFailed) ||
      (previewIsHtml && viewMode === "render"),
  );
  const htmlSafety = $derived(
    previewIsHtml && preview?.content ? analyzeHtmlSafety(preview.content) : null,
  );

  $effect(() => {
    const path = entry.path;
    const request = ++token;
    findOpen = false;
    markdownFailed = false;

    const slowTimer = setTimeout(() => {
      if (request === token) slow = true;
    }, SLOW_MS);
    const debounceTimer = setTimeout(async () => {
      try {
        const [result, app] = await Promise.all([
          invoke<FilePreview>("read_file_preview", { path }),
          entry.is_directory || isRemotePath(path) ? null : fetchDefaultApp(path).catch(() => null),
        ]);
        if (request !== token) return;
        preview = result;
        defaultApp = app;
        previewPath = path;
        error = "";
      } catch (reason) {
        if (request !== token) return;
        preview = null;
        defaultApp = null;
        previewPath = path;
        error = reason instanceof Error ? reason.message : String(reason);
      } finally {
        if (request === token) {
          clearTimeout(slowTimer);
          slow = false;
        }
      }
    }, DEBOUNCE_MS);

    return () => {
      clearTimeout(debounceTimer);
      clearTimeout(slowTimer);
      token += 1;
      slow = false;
    };
  });

  function setViewMode(mode: "render" | "code") {
    viewMode = mode;
    sessionViewMode = mode;
    findOpen = false;
  }

  function adjustHtmlZoom(amount: number) {
    htmlZoom = Math.min(MAX_HTML_ZOOM, Math.max(MIN_HTML_ZOOM, Math.round((htmlZoom + amount) * 100) / 100));
  }

  function handleKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && !event.altKey) {
      const next = fontSizeForShortcut(fontSize, event.key);
      if (next !== null) {
        event.preventDefault();
        fontSize = next;
        localStorage.setItem(FONT_SIZE_KEY, String(next));
        return;
      }
    }
    if (event.key.toLowerCase() !== "f" || !(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey) return;
    if (preview?.kind !== "text" || !preview.content) return;
    event.preventDefault();
    if (!showRendered && codeView?.openFind()) return;
    if (findOpen) findBar?.focus();
    else findOpen = true;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<aside class="flex h-full min-w-0 flex-col bg-[#242220]" aria-label="Preview">
  <div
    class="relative m-3 mb-0 flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg bg-[#1f1d1b] transition-opacity duration-150 {slow
      ? 'opacity-60'
      : ''}">
    {#if error}
      <div class="grid flex-1 place-content-center justify-items-center gap-1 px-4 text-center">
        <FileIcon class="mb-2 size-12 stroke-[1.2] text-[#67635f]" />
        <p class="text-[14px] font-semibold text-[#e8e5e2]">Can’t preview this file</p>
        <p class="text-[12px] break-all text-[#9c9895]">{error}</p>
      </div>
    {:else if !preview}
      <div class="flex-1"></div>
    {:else if preview.kind === "directory"}
      <div class="grid flex-1 place-items-center"><FolderIcon class="size-20 stroke-[1.2] text-blue-400" /></div>
    {:else if preview.kind === "binary"}
      <BinaryView name={preview.name} />
    {:else if preview.kind === "image"}
      <ImageView src={preview.src ?? ""} name={preview.name} />
    {:else if !preview.content}
      <div class="grid flex-1 place-items-center text-[13px] text-[#9c9895]">Empty file</div>
    {:else}
      {#if preview.truncated}
        <p class="shrink-0 border-b border-[#3a3734] px-3 py-1 text-[11px] text-[#9c9895]">
          Large file - showing first 2 MB
        </p>
      {/if}
      <div
        bind:this={contentRoot}
        class="relative min-h-0 flex-1"
        style="--preview-font-size: {fontSize}px; --preview-line-height: {lineHeightFor(fontSize)}px">
        {#if showRendered && previewIsMarkdown}
          <MarkdownView source={preview.content ?? ""} onError={() => (markdownFailed = true)} />
        {:else if showRendered && previewIsHtml}
          <HtmlView source={preview.content ?? ""} zoom={htmlZoom} />
        {:else}
          <CodeView
            bind:this={codeView}
            value={preview.content ?? ""}
            language={languageFor(preview.name)}
            modelKey={previewPath}
            modified={preview.modified}
            {fontSize} />
        {/if}
      </div>
      {#if findOpen && contentRoot}
        <FindBar bind:this={findBar} root={contentRoot} onClose={() => (findOpen = false)} />
      {/if}
      {#if showViewToggle}
        <div
          class="absolute right-2 bottom-2 z-10 flex items-center gap-1 rounded-md border border-[#3a3734] bg-[#2d2a28] p-0.5 text-[11px]"
          role="group"
          aria-label="Preview view">
          {#if htmlSafety}
            <span
              class="flex items-center gap-1 px-1.5 {htmlSafety.level === 'safe'
                ? 'text-emerald-400'
                : 'text-amber-400'}"
              title={htmlSafety.reasons.join("\n") || "No active content"}>
              {#if htmlSafety.level === 'safe'}
                <ShieldCheck class="size-3.5" />
                <span>Safe</span>
              {:else}
                <ShieldAlert class="size-3.5" />
                <span>Caution</span>
              {/if}
            </span>
          {/if}
          {#if previewIsHtml && showRendered}
            <button
              class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836]"
              aria-label="Zoom out"
              disabled={htmlZoom === MIN_HTML_ZOOM}
              onclick={() => adjustHtmlZoom(-0.1)}>−</button>
            <span class="min-w-9 text-center text-[#9c9895]" aria-live="polite">{Math.round(htmlZoom * 100)}%</span>
            <button
              class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836]"
              aria-label="Zoom in"
              disabled={htmlZoom === MAX_HTML_ZOOM}
              onclick={() => adjustHtmlZoom(0.1)}>+</button>
          {/if}
          <button
            class="rounded border-0 px-2 py-0.5 {viewMode === 'render'
              ? 'bg-[#3b3836] text-[#e8e5e2]'
              : 'bg-transparent text-[#9c9895] hover:text-[#e8e5e2]'}"
            aria-pressed={viewMode === "render"}
            onclick={() => setViewMode("render")}>Render</button>
          <button
            class="rounded border-0 px-2 py-0.5 {viewMode === 'code'
              ? 'bg-[#3b3836] text-[#e8e5e2]'
              : 'bg-transparent text-[#9c9895] hover:text-[#e8e5e2]'}"
            aria-pressed={viewMode === "code"}
            onclick={() => setViewMode("code")}>Code</button>
        </div>
      {/if}
    {/if}
  </div>
  <FileInfo name={entry.name} {preview} {defaultApp} {pending} />
</aside>
