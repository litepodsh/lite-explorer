<script lang="ts">
  import RecordGrid from "./record-grid.svelte";
  import { openArrow, type ArrowPreview } from "./arrow.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let arrow = $state.raw<ArrowPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    arrow = null;
    error = "";
    openArrow(target)
      .then((result) => {
        if (request === token) arrow = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0" aria-label={`Arrow preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if arrow}
    <RecordGrid schema={arrow.schema} columns={arrow.columns} rows={arrow.rows} truncated={arrow.truncated} />
  {/if}
</div>
