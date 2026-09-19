<script lang="ts">
  import RecordGrid from "./record-grid.svelte";
  import { openParquet, type ParquetPreview } from "./parquet.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let parquet = $state.raw<ParquetPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    parquet = null;
    error = "";
    openParquet(target)
      .then((result) => {
        if (request === token) parquet = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0" aria-label={`Parquet preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if parquet}
    <RecordGrid schema={parquet.schema} columns={parquet.columns} rows={parquet.rows} truncated={parquet.truncated} />
  {/if}
</div>
