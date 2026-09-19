<script lang="ts">
  import { parseCsv } from "./csv.js";
  import DataGrid from "./data-grid.svelte";

  type Props = { content: string };
  let { content }: Props = $props();

  const parsed = $derived(parseCsv(content));
  const header = $derived(parsed.rows[0] ?? []);
  const body = $derived(parsed.rows.slice(1));
  const columns = $derived(parsed.rows.reduce((max, row) => Math.max(max, row.length), 0));
  const notice = $derived(
    parsed.truncated ? `Large file - showing first ${parsed.rows.length.toLocaleString()} rows` : "",
  );
</script>

<DataGrid rows={body} {header} columnCount={columns} firstRowNumber={2} rainbow {notice} />
