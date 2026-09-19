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

<div class="flex h-full min-h-0 flex-col bg-[#1f1d1b] text-[#e8e5e2]">
  <details class="shrink-0 border-b border-[#3a3734]">
    <summary class="cursor-pointer px-3 py-1.5 text-[11.5px] text-[#9c9895] select-none">Esquema</summary>
    {#if schemaLanguage}
      <NotebookCode source={schema} language={schemaLanguage} />
    {:else}
      <pre class="max-h-52 overflow-auto bg-[#191817] px-3 py-2 font-mono text-[11.5px] leading-relaxed text-[#c0bbb5]">{schema}</pre>
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
