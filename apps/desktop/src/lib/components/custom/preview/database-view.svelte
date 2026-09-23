<script lang="ts">
  import DatabaseIcon from "@lucide/svelte/icons/database";
  import TableIcon from "@lucide/svelte/icons/table";
  import DataGrid from "./data-grid.svelte";
  import {
    openDatabase,
    readSqliteTable,
    type DatabasePreview,
    type TableData,
  } from "./database.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let database = $state.raw<DatabasePreview | null>(null);
  let selected = $state("");
  let table = $state.raw<TableData | null>(null);
  let error = $state("");
  let token = 0;

  function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    database = null;
    selected = "";
    table = null;
    error = "";
    openDatabase(target)
      .then((result) => {
        if (request !== token) return;
        database = result;
        if (result.tables.length > 0) void select(result.tables[0].name);
      })
      .catch((reason) => {
        if (request === token) error = message(reason);
      });
  });

  async function select(name: string) {
    const request = token;
    selected = name;
    table = null;
    try {
      const result = await readSqliteTable(path, name);
      if (request === token) table = result;
    } catch (reason) {
      if (request === token) error = message(reason);
    }
  }

  const header = $derived(table?.columns.map((column) => column.name) ?? []);
  const columnCount = $derived(table?.columns.length ?? 0);
</script>

<div class="flex h-full min-h-0 bg-[var(--app-input)]" aria-label={`Database preview of ${name}`}>
  {#if error && !database}
    <div class="grid h-full flex-1 place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[var(--app-fg-muted)]">{error}</p>
    </div>
  {:else if database}
    <aside class="flex w-56 shrink-0 flex-col border-r border-[var(--app-border)] bg-[var(--app-bg)]">
      <p class="flex shrink-0 items-center gap-1.5 border-b border-[var(--app-border)] px-3 py-2 text-[11px] text-[var(--app-fg-muted)]">
        <DatabaseIcon class="size-3.5" />{database.tables.length} tablas
      </p>
      <div class="min-h-0 flex-1 overflow-auto">
        {#each database.tables as item (item.name)}
          <button
            type="button"
            class="flex w-full items-center gap-2 border-0 px-3 py-2 text-left text-[12.5px] {item.name === selected
              ? 'bg-[var(--app-surface-raised)] text-[var(--app-fg)]'
              : 'bg-transparent text-[var(--app-fg)] hover:bg-[var(--app-surface)]'}"
            onclick={() => void select(item.name)}>
            <TableIcon class="size-3.5 shrink-0 text-[var(--app-fg-faint)]" />
            <span class="min-w-0 flex-1 truncate">{item.name}</span>
            <span class="shrink-0 text-[10.5px] text-[var(--app-fg-faint)]">{item.rows}</span>
          </button>
        {/each}
      </div>
    </aside>
    <div class="min-h-0 min-w-0 flex-1">
      {#if error}
        <p class="px-3 py-2 text-[12.5px] text-[var(--app-fg-muted)]">{error}</p>
      {:else if table}
        <DataGrid
          rows={table.rows}
          {columnCount}
          {header}
          firstRowNumber={1}
          notice={table.truncated ? "Mostrando las primeras 500 filas" : ""}
          emptyLabel="Tabla vacía" />
      {:else}
        <div class="grid h-full place-items-center">
          <span class="size-6 animate-spin rounded-full border-2 border-[color-mix(in_srgb,var(--app-fg)_20%,transparent)] border-t-[var(--app-fg)]" aria-label="Loading"></span>
        </div>
      {/if}
    </div>
  {/if}
</div>
