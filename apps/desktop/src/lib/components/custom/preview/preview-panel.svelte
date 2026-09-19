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
  import CsvView from "./csv-view.svelte";
  import FileInfo from "./file-info.svelte";
  import FindBar from "./find-bar.svelte";
  import MarkdownView from "./markdown-view.svelte";
  import HtmlView from "./html-view.svelte";
  import ImageView from "./image-view.svelte";
  import BinaryView from "./binary-view.svelte";
  import PdfView from "./pdf-view.svelte";
  import MediaView from "./media-view.svelte";
  import FontView from "./font-view.svelte";
  import EpubView from "./epub-view.svelte";
  import SpreadsheetView from "./spreadsheet-view.svelte";
  import RtfView from "./rtf-view.svelte";
  import WordView from "./word-view.svelte";
  import PresentationView from "./presentation-view.svelte";
  import MailView from "./mail-view.svelte";
  import MboxView from "./mbox-view.svelte";
  import ContactView from "./contact-view.svelte";
  import CalendarView from "./calendar-view.svelte";
  import TorrentView from "./torrent-view.svelte";
  import DataView from "./data-view.svelte";
  import DiffView from "./diff-view.svelte";
  import LogView from "./log-view.svelte";
  import ComicView from "./comic-view.svelte";
  import NotebookView from "./notebook-view.svelte";
  import DatabaseView from "./database-view.svelte";
  import SubtitleView from "./subtitle-view.svelte";
  import CertificateView from "./certificate-view.svelte";
  import ModelView from "./model-view.svelte";
  import GeoView from "./geo-view.svelte";
  import Fb2View from "./fb2-view.svelte";
  import PcapView from "./pcap-view.svelte";
  import IsoView from "./iso-view.svelte";
  import MsgView from "./msg-view.svelte";
  import SketchView from "./sketch-view.svelte";
  import PsdView from "./psd-view.svelte";
  import DicomView from "./dicom-view.svelte";
  import MobiView from "./mobi-view.svelte";
  import AvroView from "./avro-view.svelte";
  import ParquetView from "./parquet-view.svelte";
  import ArrowView from "./arrow-view.svelte";
  import { fetchMediaUrl, openViewer } from "./media.js";
  import ArchiveView from "$lib/archive/archive-view.svelte";
  import { fontSizeForShortcut, lineHeightFor, parseFontSize } from "./font-size.js";
  import { isCsvName, isHtmlName, isMarkdownName, isMediaKind, languageFor } from "./languages.js";
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
  let mediaUrl = $state("");
  let error = $state("");
  let slow = $state(false);
  let findOpen = $state(false);
  let viewMode = $state(sessionViewMode);
  let markdownFailed = $state(false);
  let fontSize = $state(parseFontSize(localStorage.getItem(FONT_SIZE_KEY)));
  let htmlZoom = $state(1);
  let contentRoot = $state<HTMLElement | null>(null);
  let codeView = $state<ReturnType<typeof CodeView> | null>(null);
  let pdfView = $state<ReturnType<typeof PdfView> | null>(null);
  let mediaView = $state<ReturnType<typeof MediaView> | null>(null);
  let findBar = $state<ReturnType<typeof FindBar> | null>(null);
  let token = 0;

  // Content may show the previous file while a request is pending; file
  // information hides stale values but keeps their space to avoid a jump.
  const pending = $derived(previewPath !== entry.path);
  const previewIsMarkdown = $derived(preview ? isMarkdownName(preview.name) : false);
  const previewIsHtml = $derived(preview ? isHtmlName(preview.name) : false);
  const previewIsCsv = $derived(preview ? isCsvName(preview.name) : false);
  const showViewToggle = $derived(previewIsMarkdown || previewIsHtml || previewIsCsv);
  const showRendered = $derived(
    (previewIsMarkdown && viewMode === "render" && !markdownFailed) ||
      (previewIsHtml && viewMode === "render") ||
      (previewIsCsv && viewMode === "render"),
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
        // Media is streamed: ask for a range-served URL before rendering the view.
        mediaUrl = "";
        if (isMediaKind(result.kind)) {
          const media = await fetchMediaUrl(path);
          if (request !== token) return;
          mediaUrl = media.url;
        }
      } catch (reason) {
        if (request !== token) return;
        preview = null;
        defaultApp = null;
        previewPath = path;
        mediaUrl = "";
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
    if (preview?.kind === "pdf") {
      if (pdfView?.openFind()) event.preventDefault();
      return;
    }
    if (preview?.kind !== "text" || !preview.content || !codeView?.hasTextFocus()) return;
    event.preventDefault();
    void codeView.openFind();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#snippet mediaSpinner()}
  <div class="grid flex-1 place-items-center">
    <span class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70" aria-label="Loading"></span>
  </div>
{/snippet}

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
    {:else if preview.kind === "archive"}
      <ArchiveView path={previewPath} name={preview.name} />
    {:else if preview.kind === "binary"}
      <BinaryView name={preview.name} />
    {:else if preview.kind === "pdf"}
      {#if mediaUrl}
        <PdfView
          bind:this={pdfView}
          src={mediaUrl}
          name={preview.name}
          onMaximize={() => openViewer(previewPath)} />
      {:else}
        {@render mediaSpinner()}
      {/if}
    {:else if preview.kind === "image"}
      {#if mediaUrl}
        <ImageView src={mediaUrl} name={preview.name} onMaximize={() => openViewer(previewPath)} />
      {:else}
        {@render mediaSpinner()}
      {/if}
    {:else if preview.kind === "video" || preview.kind === "audio"}
      {#if mediaUrl}
        <MediaView
          bind:this={mediaView}
          src={mediaUrl}
          name={preview.name}
          kind={preview.kind}
          path={previewPath}
          onMaximize={() => {
            mediaView?.pause();
            void openViewer(previewPath);
          }} />
      {:else}
        {@render mediaSpinner()}
      {/if}
    {:else if preview.kind === "font"}
      {#if mediaUrl}
        <FontView src={mediaUrl} name={preview.name} />
      {:else}
        {@render mediaSpinner()}
      {/if}
    {:else if preview.kind === "epub"}
      <EpubView path={previewPath} name={preview.name} />
    {:else if preview.kind === "spreadsheet"}
      <SpreadsheetView path={previewPath} name={preview.name} />
    {:else if preview.kind === "rtf"}
      <RtfView path={previewPath} name={preview.name} />
    {:else if preview.kind === "word"}
      <WordView path={previewPath} name={preview.name} />
    {:else if preview.kind === "presentation"}
      <PresentationView path={previewPath} name={preview.name} />
    {:else if preview.kind === "mail"}
      <MailView path={previewPath} name={preview.name} />
    {:else if preview.kind === "mbox"}
      <MboxView path={previewPath} name={preview.name} />
    {:else if preview.kind === "contact"}
      <ContactView path={previewPath} name={preview.name} />
    {:else if preview.kind === "calendar"}
      <CalendarView path={previewPath} name={preview.name} />
    {:else if preview.kind === "torrent"}
      <TorrentView path={previewPath} name={preview.name} />
    {:else if preview.kind === "data"}
      <DataView path={previewPath} name={preview.name} />
    {:else if preview.kind === "diff"}
      <DiffView content={preview.content ?? ""} />
    {:else if preview.kind === "log"}
      <LogView content={preview.content ?? ""} />
    {:else if preview.kind === "comic"}
      <ComicView path={previewPath} name={preview.name} />
    {:else if preview.kind === "notebook"}
      <NotebookView path={previewPath} name={preview.name} />
    {:else if preview.kind === "database"}
      <DatabaseView path={previewPath} name={preview.name} />
    {:else if preview.kind === "subtitle"}
      <SubtitleView path={previewPath} name={preview.name} />
    {:else if preview.kind === "certificate"}
      <CertificateView path={previewPath} name={preview.name} />
    {:else if preview.kind === "model"}
      {#if mediaUrl}
        <ModelView src={mediaUrl} name={preview.name} />
      {:else}
        {@render mediaSpinner()}
      {/if}
    {:else if preview.kind === "geo"}
      <GeoView path={previewPath} name={preview.name} />
    {:else if preview.kind === "fb2"}
      <Fb2View path={previewPath} name={preview.name} />
    {:else if preview.kind === "pcap"}
      <PcapView path={previewPath} name={preview.name} />
    {:else if preview.kind === "iso"}
      <IsoView path={previewPath} name={preview.name} />
    {:else if preview.kind === "msg"}
      <MsgView path={previewPath} name={preview.name} />
    {:else if preview.kind === "sketch"}
      <SketchView path={previewPath} name={preview.name} />
    {:else if preview.kind === "psd"}
      <PsdView path={previewPath} name={preview.name} />
    {:else if preview.kind === "dicom"}
      <DicomView path={previewPath} name={preview.name} />
    {:else if preview.kind === "mobi"}
      <MobiView path={previewPath} name={preview.name} />
    {:else if preview.kind === "avro"}
      <AvroView path={previewPath} name={preview.name} />
    {:else if preview.kind === "parquet"}
      <ParquetView path={previewPath} name={preview.name} />
    {:else if preview.kind === "arrow"}
      <ArrowView path={previewPath} name={preview.name} />
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
        {:else if showRendered && previewIsCsv}
          <CsvView content={preview.content ?? ""} />
        {:else}
          <CodeView
            bind:this={codeView}
            value={preview.content ?? ""}
            language={languageFor(preview.name)}
            modelKey={previewPath}
            modified={preview.modified}
            csv={previewIsCsv}
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
            onclick={() => setViewMode("render")}>{previewIsCsv ? "Table" : "Render"}</button>
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
