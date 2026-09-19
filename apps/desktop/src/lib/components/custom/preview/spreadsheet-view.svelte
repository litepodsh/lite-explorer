<script lang="ts">
  import FileIcon from "@lucide/svelte/icons/file";
  import DataGrid from "./data-grid.svelte";
  import { readSpreadsheet, type SpreadsheetData } from "./spreadsheet.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let data = $state.raw<SpreadsheetData | null>(null);
  let active = $state(0);
  let error = $state("");
  let loading = $state(false);
  let token = 0;

  const sheet = $derived(data?.sheets[active] ?? null);
  const columns = $derived(
    sheet ? sheet.rows.reduce((max, row) => Math.max(max, row.length), 0) : 0,
  );
  const notice = $derived(
    sheet?.truncated
      ? `Sheet truncated to ${sheet.rows.length.toLocaleString()} rows`
      : data?.truncated
        ? `Showing the first ${data.sheets.length.toLocaleString()} sheets`
        : "",
  );

  $effect(() => {
    const target = path;
    const request = ++token;
    data = null;
    active = 0;
    error = "";
    loading = true;
    readSpreadsheet(target)
      .then((result) => {
        if (request !== token) return;
        if (result.sheets.length === 0) {
          error = "This workbook has no sheets";
          return;
        }
        data = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      })
      .finally(() => {
        if (request === token) loading = false;
      });
  });
</script>

<div class="flex h-full min-h-0 flex-col" aria-label={`Spreadsheet preview of ${name}`}>
  {#if error}
    <div class="grid flex-1 place-content-center justify-items-center gap-1 px-4 text-center">
      <FileIcon class="mb-2 size-12 stroke-[1.2] text-[#67635f]" />
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if loading && !data}
    <div class="grid flex-1 place-items-center">
      <span
        class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70"
        aria-label="Loading"></span>
    </div>
  {:else if sheet}
    {#if data && data.sheets.length > 1}
      <div
        class="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-[#3a3734] px-2 py-1.5"
        role="tablist"
        aria-label="Sheets">
        {#each data.sheets as item, index (item.name)}
          <button
            type="button"
            role="tab"
            aria-selected={index === active}
            class="shrink-0 rounded px-2.5 py-1 text-[12px] {index === active
              ? 'bg-[#3b3836] text-[#e8e5e2]'
              : 'bg-transparent text-[#9c9895] hover:bg-[#2f2c29] hover:text-[#e8e5e2]'}"
            onclick={() => (active = index)}>
            {item.name}
          </button>
        {/each}
      </div>
    {/if}
    <div class="min-h-0 flex-1">
      <DataGrid rows={sheet.rows} columnCount={columns} {notice} emptyLabel="Empty sheet" />
    </div>
  {/if}
</div>
