<script lang="ts">
  import RecordGrid from "./record-grid.svelte";
  import { openAvro, type AvroPreview } from "./avro.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let avro = $state.raw<AvroPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    avro = null;
    error = "";
    openAvro(target)
      .then((result) => {
        if (request === token) avro = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0" aria-label={`Avro preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if avro}
    <RecordGrid schema={avro.schema} columns={avro.columns} rows={avro.rows} truncated={avro.truncated} schemaLanguage="json" />
  {/if}
</div>
