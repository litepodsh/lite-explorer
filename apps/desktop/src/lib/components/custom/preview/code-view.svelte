<script lang="ts">
  import { onMount } from "svelte";
  import type * as Monaco from "monaco-editor/editor/editor.api";
  import { DEFAULT_FONT_SIZE, lineHeightFor } from "./font-size.js";
  import { ensureLanguage, loadMonaco, modelFor, previewTheme, type CachedModel, type MonacoApi } from "./monaco.js";
  import { detectDelimiter, sampleLines, visibleDecorations } from "./rainbow-csv.js";

  type Props = {
    value: string;
    language: string;
    modelKey: string;
    modified?: number | null;
    fontSize?: number;
    csv?: boolean;
  };
  let { value, language, modelKey, modified = null, fontSize = DEFAULT_FONT_SIZE, csv = false }: Props = $props();

  // Lines above/below the viewport to color ahead of scroll, plus a hard cap so a
  // not-yet-laid-out editor (which can report the whole document as visible)
  // cannot trigger a huge scan.
  const RAINBOW_OVERSCAN = 40;
  const MAX_VISIBLE_LINES = 800;

  let host: HTMLDivElement;
  let api = $state.raw<MonacoApi | null>(null);
  let editor = $state.raw<Monaco.editor.IStandaloneCodeEditor | null>(null);
  let current: CachedModel | null = null;
  let rainbow: Monaco.editor.IEditorDecorationsCollection | null = null;
  let listeners: Monaco.IDisposable[] = [];
  let delimiterModel: Monaco.editor.ITextModel | null = null;
  let delimiterVersion = -1;
  let delimiter = ",";
  let rainbowFrame = 0;

  function scheduleRainbow() {
    cancelAnimationFrame(rainbowFrame);
    rainbowFrame = requestAnimationFrame(paintRainbow);
  }

  // Delimiter detection is sampled once per model revision, not per repaint.
  function delimiterFor(model: Monaco.editor.ITextModel): string {
    const version = model.getVersionId();
    if (model === delimiterModel && version === delimiterVersion) return delimiter;
    delimiter = detectDelimiter(sampleLines(model));
    delimiterModel = model;
    delimiterVersion = version;
    return delimiter;
  }

  // Only visible lines are scanned, so cost is independent of file size.
  function paintRainbow() {
    if (!editor || !rainbow) return;
    if (!csv) {
      rainbow.clear();
      return;
    }
    const model = editor.getModel();
    if (!model) return;

    const lineCount = model.getLineCount();
    const lines = new Set<number>();
    for (const range of editor.getVisibleRanges()) {
      const start = Math.max(1, range.startLineNumber - RAINBOW_OVERSCAN);
      const end = Math.min(lineCount, range.endLineNumber + RAINBOW_OVERSCAN);
      for (let line = start; line <= end && lines.size < MAX_VISIBLE_LINES; line++) lines.add(line);
      if (lines.size >= MAX_VISIBLE_LINES) break;
    }
    rainbow.set(visibleDecorations(model, delimiterFor(model), lines));
  }

  onMount(() => {
    let disposed = false;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => editor?.layout());
    });
    observer.observe(host);
    const themeObserver = new MutationObserver(() => api?.editor.setTheme(previewTheme()));
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

    loadMonaco()
      .then((monaco) => {
        if (disposed) return;
        api = monaco;
        const instance = monaco.editor.create(host, {
          theme: previewTheme(),
          readOnly: true,
          domReadOnly: true,
          minimap: { enabled: false },
          renderLineHighlight: "none",
          occurrencesHighlight: "off",
          quickSuggestions: false,
          folding: false,
          wordWrap: "on",
          scrollBeyondLastLine: false,
          automaticLayout: false,
          contextmenu: false,
          fontSize,
          lineHeight: lineHeightFor(fontSize),
          fontFamily: 'ui-monospace, "SF Mono", Menlo, Consolas, monospace',
          padding: { top: 8, bottom: 8 },
        });
        editor = instance;
        rainbow = instance.createDecorationsCollection([]);
        listeners = [
          instance.onDidScrollChange(scheduleRainbow),
          instance.onDidChangeModel(() => {
            delimiterModel = null;
            scheduleRainbow();
          }),
          instance.onDidChangeModelContent(scheduleRainbow),
        ];
      })
      .catch(() => {
        // Keep the plain-text fallback.
      });

    return () => {
      disposed = true;
      observer.disconnect();
      themeObserver.disconnect();
      cancelAnimationFrame(frame);
      cancelAnimationFrame(rainbowFrame);
      for (const listener of listeners) listener.dispose();
      listeners = [];
      if (editor && current) current.viewState = editor.saveViewState();
      editor?.dispose();
    };
  });

  $effect(() => {
    // Re-paint when the rendered flag, content or editor instance changes.
    void value;
    void csv;
    if (!editor) return;
    if (!csv) {
      cancelAnimationFrame(rainbowFrame);
      rainbow?.clear();
      delimiterModel = null;
      return;
    }
    scheduleRainbow();
  });

  $effect(() => {
    if (!api || !editor) return;
    const next = modelFor(api, modelKey, value, language, modified);
    // Monaco retokenizes open models once the grammar registers.
    ensureLanguage(language).catch(() => {});
    if (next === current) return;
    if (current) current.viewState = editor.saveViewState();
    editor.setModel(next.model);
    if (next.viewState) editor.restoreViewState(next.viewState);
    current = next;
  });

  $effect(() => {
    editor?.updateOptions({ fontSize, lineHeight: lineHeightFor(fontSize) });
  });

  export function openFind(): boolean {
    if (!editor) return false;
    editor.focus();
    void editor.getAction("actions.find")?.run();
    return true;
  }
  export function hasTextFocus(): boolean { return editor?.hasTextFocus() ?? false; }
</script>

<div class="relative h-full min-h-0 w-full">
  <div bind:this={host} class="absolute inset-0" class:invisible={!editor}></div>
  {#if !editor}
    <pre
      class="absolute inset-0 m-0 overflow-auto p-3 font-mono whitespace-pre-wrap break-words text-[var(--app-fg)]"
      style="font-size: {fontSize}px; line-height: {lineHeightFor(fontSize)}px">{value}</pre>
  {/if}
</div>
