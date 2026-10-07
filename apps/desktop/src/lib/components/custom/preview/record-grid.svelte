<script lang="ts">
  import DataGrid from "./data-grid.svelte";
  import NotebookCode from "./notebook-code.svelte";

  type Props = {
    schema: string;
    columns: string[];
    rows: string[][];
    truncated: boolean;
    /** When set, the schema is highlighted with that language. */
    schemaLanguage?: string;
  };
  let { schema, columns, rows, truncated, schemaLanguage = "" }: Props = $props();
</script>

<div class="flex h-full min-h-0 flex-col bg-[var(--app-input)] text-[var(--app-fg)]">
  <details class="shrink-0 border-b border-[var(--app-border)]">
    <summary class="cursor-pointer px-3 py-1.5 text-[11.5px] text-[var(--app-fg-muted)] select-none">Esquema</summary>
    {#if schemaLanguage}
      <NotebookCode source={schema} language={schemaLanguage} />
    {:else}
      <pre class="max-h-52 overflow-auto bg-[var(--app-input)] px-3 py-2 font-mono text-[11.5px] leading-relaxed text-[var(--app-fg-muted)]">{schema}</pre>
    {/if}
  </details>
  <div class="min-h-0 flex-1">
    <DataGrid
      {rows}
      columnCount={columns.length}
      header={columns}
      firstRowNumber={1}
      notice={truncated ? "Mostrando las primeras 500 filas" : ""}
      emptyLabel="Sin registros" />
  </div>
</div>
