<script lang="ts">
  import { onMount } from "svelte";
  import type * as Monaco from "monaco-editor/editor/editor.api";
  import { DEFAULT_FONT_SIZE, lineHeightFor } from "./font-size.js";
  import { ensureLanguage, loadMonaco, modelFor, PREVIEW_THEME, type CachedModel, type MonacoApi } from "./monaco.js";

  type Props = { value: string; language: string; modelKey: string; modified?: number | null; fontSize?: number };
  let { value, language, modelKey, modified = null, fontSize = DEFAULT_FONT_SIZE }: Props = $props();

  let host: HTMLDivElement;
  let api = $state.raw<MonacoApi | null>(null);
  let editor = $state.raw<Monaco.editor.IStandaloneCodeEditor | null>(null);
  let current: CachedModel | null = null;

  onMount(() => {
    let disposed = false;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => editor?.layout());
    });
    observer.observe(host);

    loadMonaco()
      .then((monaco) => {
        if (disposed) return;
        api = monaco;
        editor = monaco.editor.create(host, {
          theme: PREVIEW_THEME,
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
      })
      .catch(() => {
        // Keep the plain-text fallback.
      });

    return () => {
      disposed = true;
      observer.disconnect();
      cancelAnimationFrame(frame);
      if (editor && current) current.viewState = editor.saveViewState();
      editor?.dispose();
    };
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
</script>

<div class="relative h-full min-h-0 w-full">
  <div bind:this={host} class="absolute inset-0" class:invisible={!editor}></div>
  {#if !editor}
    <pre
      class="absolute inset-0 m-0 overflow-auto p-3 font-mono whitespace-pre-wrap break-words text-[#e8e5e2]"
      style="font-size: {fontSize}px; line-height: {lineHeightFor(fontSize)}px">{value}</pre>
  {/if}
</div>
