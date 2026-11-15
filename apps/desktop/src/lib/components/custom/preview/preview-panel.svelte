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
  import CopyIcon from "@lucide/svelte/icons/copy";
  import TextCursorIcon from "@lucide/svelte/icons/text-cursor";
  import ClipboardCopyIcon from "@lucide/svelte/icons/clipboard-copy";
  import Link2Icon from "@lucide/svelte/icons/link-2";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import type { DirectoryEntry } from "#lib/components/custom/file-list/list-item.svelte";
  import CodeView from "./code-view.svelte";
  import CsvView from "./csv-view.svelte";
  import FileInfo from "./file-info.svelte";
  import FindBar from "./find-bar.svelte";
  import MarkdownView from "./markdown-view.svelte";
  import HtmlView from "./html-view.svelte";
  import ImageView from "./image-view.svelte";
  import BinaryView from "./binary-view.svelte";
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
  import DuckdbView from "./duckdb-view.svelte";
  import SubtitleView from "./subtitle-view.svelte";
  import CertificateView from "./certificate-view.svelte";
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
  import ArchiveView from "#lib/archive/archive-view.svelte";
  import { fontSizeForShortcut, lineHeightFor, parseFontSize } from "./font-size.js";
  import { isCsvName, isDataName, isHtmlName, isMarkdownName, isMediaKind, languageFor } from "./languages.js";
  import { analyzeHtmlSafety } from "./html-safety.js";
  import { formatHtml } from "./format-html.js";
  import type { FilePreview } from "./types.js";
  import { fetchDefaultApp, type OpenWithApp } from "#lib/file-ops/open.js";
  import { kindLabel } from "./languages.js";
  import { formatSize } from "./format.js";
  import { isRemotePath } from "#lib/remote/remote-locations.js";

  type Props = { entry: DirectoryEntry };
  type PdfViewComponent = typeof import("./pdf-view.svelte").default;
  type ModelViewComponent = typeof import("./model-view.svelte").default;
  let { entry }: Props = $props();

  const DEBOUNCE_MS = 150;
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
  let PdfView = $state<PdfViewComponent | null>(null);
  let ModelView = $state<ModelViewComponent | null>(null);
  let pdfView = $state<ReturnType<PdfViewComponent> | null>(null);
  let mediaView = $state<ReturnType<typeof MediaView> | null>(null);
  let findBar = $state<ReturnType<typeof FindBar> | null>(null);
  let stageEl = $state<HTMLElement | null>(null);
  let menuSelection = $state("");
  /** Pretty printed HTML for the code view, keyed by the source it came from. */
  let prettyHtml = $state.raw<{ source: string; text: string } | null>(null);
  let token = 0;

  // Content may show the previous file while a request is pending; file
  // information hides stale values but keeps their space to avoid a jump.
  const pending = $derived(previewPath !== entry.path);
  const previewIsMarkdown = $derived(preview ? isMarkdownName(preview.name) : false);
  const previewIsHtml = $derived(preview ? isHtmlName(preview.name) : false);
  const previewIsCsv = $derived(preview ? isCsvName(preview.name) : false);
  const previewIsData = $derived(preview ? isDataName(preview.name) : false);
  const showViewToggle = $derived(previewIsMarkdown || previewIsHtml || previewIsCsv || previewIsData);
  const showRendered = $derived(
    (previewIsMarkdown && viewMode === "render" && !markdownFailed) ||
      (previewIsHtml && viewMode === "render") ||
      (previewIsCsv && viewMode === "render") ||
      (previewIsData && viewMode === "render"),
  );
  // Folders, binaries, errors and empty files have nothing to scroll: a short stage leaves room for the details.
  const compactStage = $derived(
    Boolean(error) || !preview || preview.kind === "directory" || preview.kind === "binary" ||
      (preview.kind === "text" && !preview.content),
  );
  const htmlSafety = $derived(
    previewIsHtml && preview?.content ? analyzeHtmlSafety(preview.content) : null,
  );

  $effect(() => {
    const source = preview?.content;
    if (!previewIsHtml || showRendered || !source || preview?.truncated) return;
    if (prettyHtml?.source === source) return;
    let cancelled = false;
    void formatHtml(source).then((text) => {
      if (!cancelled && text) prettyHtml = { source, text };
    });
    return () => { cancelled = true; };
  });
  const codeText = $derived(
    previewIsHtml && prettyHtml && prettyHtml.source === preview?.content ? prettyHtml.text : (preview?.content ?? ""),
  );
  const codeIsPretty = $derived(codeText !== (preview?.content ?? ""));

  $effect(() => {
    const kind = preview?.kind;
    let cancelled = false;
    if (kind === "pdf" && !PdfView) {
      void import("./pdf-view.svelte").then(({ default: View }) => {
        if (!cancelled) PdfView = View;
      });
    }
    if (kind === "model" && !ModelView) {
      void import("./model-view.svelte").then(({ default: View }) => {
        if (!cancelled) ModelView = View;
      });
    }
    return () => { cancelled = true; };
  });

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

  /** Text selected inside the stage: Monaco's own selection, or the page's for rendered views. */
  function selectedText(): string {
    const fromEditor = codeView?.selectedText() ?? "";
    if (fromEditor) return fromEditor;
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !stageEl?.contains(selection.anchorNode)) return "";
    return selection.toString();
  }

  /** Selects the preview's text only, never the whole window. */
  function selectAllText() {
    if (codeView?.selectAll()) return;
    const root = contentRoot ?? stageEl;
    const selection = window.getSelection();
    if (!root || !selection) return;
    const range = document.createRange();
    range.selectNodeContents(root);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  async function copyText(text: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // WebKit rejects clipboard writes without a fresh user activation.
    }
  }

  function inThisPreview(target: EventTarget | null): boolean {
    const scope = target instanceof Element ? target.closest("[data-key-scope='preview']") : null;
    return Boolean(scope && stageEl && scope.contains(stageEl));
  }

  function handleKeydown(event: KeyboardEvent) {
    if (
      !event.defaultPrevented &&
      event.key.toLowerCase() === "a" &&
      (event.metaKey || event.ctrlKey) &&
      !event.shiftKey &&
      !event.altKey &&
      inThisPreview(event.target)
    ) {
      event.preventDefault();
      selectAllText();
      return;
    }
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
    <span class="size-6 animate-spin rounded-full border-2 border-[color-mix(in_srgb,var(--app-fg)_20%,transparent)] border-t-[var(--app-fg)]" aria-label="Loading"></span>
  </div>
{/snippet}

<ContextMenu.Root onOpenChange={(open) => { if (open) menuSelection = selectedText(); }}>
<ContextMenu.Trigger class="flex h-full min-h-0 min-w-0 flex-col select-text">
<aside class="preview-panel flex h-full min-w-0 flex-col" aria-label="Preview">
  <header class="preview-head">
    <span class="preview-kicker">{preview && !pending ? kindLabel(preview.name, preview.kind) : entry.is_directory ? "Folder" : "File"}</span>
    <div class="preview-head-row">
      <h2 class="preview-title" title={entry.name}>{entry.name}</h2>
      {#if preview && !pending && preview.kind !== "directory"}<span class="preview-size">{formatSize(preview.size)}</span>{/if}
    </div>
  </header>
  <div
    bind:this={stageEl}
    class="preview-stage relative flex min-h-0 flex-col overflow-hidden transition-opacity duration-150 {slow
      ? 'opacity-60'
      : ''}"
    class:compact={compactStage}>
    {#if error}
      <div class="preview-placeholder">
        <span class="preview-glyph"><FileIcon /></span>
        <p class="preview-placeholder-title">Can’t preview this file</p>
        <p class="preview-placeholder-text break-all">{error}</p>
      </div>
    {:else if !preview}
      <div class="flex-1"></div>
    {:else if preview.kind === "directory"}
      <div class="preview-placeholder"><span class="preview-glyph accent"><FolderIcon /></span></div>
    {:else if preview.kind === "archive"}
      <ArchiveView path={previewPath} name={preview.name} />
    {:else if preview.kind === "binary"}
      <BinaryView name={preview.name} />
    {:else if preview.kind === "pdf"}
      {#if mediaUrl && PdfView}
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
    {:else if preview.kind === "duckdb"}
      <DuckdbView path={previewPath} name={preview.name} />
    {:else if preview.kind === "subtitle"}
      <SubtitleView path={previewPath} name={preview.name} />
    {:else if preview.kind === "certificate"}
      <CertificateView path={previewPath} name={preview.name} />
    {:else if preview.kind === "model"}
      {#if mediaUrl && ModelView}
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
      <div class="preview-placeholder">
        <span class="preview-glyph"><FileIcon /></span>
        <p class="preview-placeholder-text">Empty file</p>
      </div>
    {:else}
      {#if preview.truncated && !(previewIsData && showRendered)}
        <p class="preview-notice">
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
        {:else if showRendered && previewIsData}
          <DataView path={previewPath} name={preview.name} />
        {:else}
          <CodeView
            bind:this={codeView}
            value={codeText}
            language={languageFor(preview.name)}
            modelKey={codeIsPretty ? `${previewPath}#pretty` : previewPath}
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
          class="preview-toggle absolute right-2 bottom-2 z-10 flex items-center gap-1 text-[11px]"
          role="group"
          aria-label="Preview view">
          {#if htmlSafety}
            <span
              class="flex items-center gap-1 px-1.5 {htmlSafety.level === 'safe'
                ? 'text-emerald-400'
                : 'text-[var(--app-warning)]'}"
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
              class="grid size-6 place-items-center rounded border-0 bg-transparent text-[var(--app-fg)] hover:bg-[var(--app-surface-raised)]"
              aria-label="Zoom out"
              disabled={htmlZoom === MIN_HTML_ZOOM}
              onclick={() => adjustHtmlZoom(-0.1)}>−</button>
            <span class="min-w-9 text-center text-[var(--app-fg-muted)]" aria-live="polite">{Math.round(htmlZoom * 100)}%</span>
            <button
              class="grid size-6 place-items-center rounded border-0 bg-transparent text-[var(--app-fg)] hover:bg-[var(--app-surface-raised)]"
              aria-label="Zoom in"
              disabled={htmlZoom === MAX_HTML_ZOOM}
              onclick={() => adjustHtmlZoom(0.1)}>+</button>
          {/if}
          <button
            class="preview-toggle-option"
            aria-pressed={viewMode === "render"}
            onclick={() => setViewMode("render")}>{previewIsData ? "Tree" : previewIsCsv ? "Table" : "Render"}</button>
          <button
            class="preview-toggle-option"
            aria-pressed={viewMode === "code"}
            onclick={() => setViewMode("code")}>Code</button>
        </div>
      {/if}
    {/if}
  </div>
  <FileInfo {preview} {defaultApp} {pending} />
</aside>
</ContextMenu.Trigger>
<ContextMenu.Content class="w-56">
  <ContextMenu.Item disabled={!menuSelection} onSelect={() => copyText(menuSelection)}>
    <CopyIcon />Copy<ContextMenu.Shortcut>⌘C</ContextMenu.Shortcut>
  </ContextMenu.Item>
  <ContextMenu.Item disabled={!preview?.content} onSelect={selectAllText}>
    <TextCursorIcon />Select All<ContextMenu.Shortcut>⌘A</ContextMenu.Shortcut>
  </ContextMenu.Item>
  <ContextMenu.Separator />
  <ContextMenu.Item disabled={!preview?.content} onSelect={() => copyText(preview?.content ?? "")}>
    <ClipboardCopyIcon />Copy All Text
  </ContextMenu.Item>
  <ContextMenu.Item onSelect={() => copyText(entry.path)}>
    <Link2Icon />Copy Path
  </ContextMenu.Item>
  <ContextMenu.Item onSelect={() => copyText(entry.name)}>
    <FileTextIcon />Copy Name
  </ContextMenu.Item>
</ContextMenu.Content>
</ContextMenu.Root>
