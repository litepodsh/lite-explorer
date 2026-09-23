<script lang="ts">
  import { onMount } from "svelte";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import CheckIcon from "@lucide/svelte/icons/check";
  import { RAINBOW_COLUMN_COUNT } from "./rainbow-csv.js";

  type Props = {
    /** Data rows, without any header. */
    rows: string[][];
    /** Columns to render; cells past a row's length are blank. */
    columnCount: number;
    /** Optional sticky header; its presence numbers the first data row at `firstRowNumber`. */
    header?: string[];
    firstRowNumber?: number;
    /** Colors each column, for delimited data. */
    rainbow?: boolean;
    /** Banner shown above the grid, e.g. when rows were capped. */
    notice?: string;
    emptyLabel?: string;
  };
  let {
    rows,
    columnCount,
    header,
    firstRowNumber = 1,
    rainbow = false,
    notice = "",
    emptyLabel = "Empty file",
  }: Props = $props();

  // Fixed row height lets the table window rows without measuring, so a 50k-row
  // file keeps ~30 DOM rows regardless of scroll position.
  const ROW_HEIGHT = 30;
  const OVERSCAN = 8;
  const GUTTER = "3.5rem";
  const COLUMN_MIN = "9rem";

  const hasHeader = $derived(!!header && header.length > 0);
  const columns = $derived(Math.max(columnCount, 1));

  let root = $state<HTMLElement | null>(null);
  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(0);
  let selection = $state<{ x: number; y: number } | null>(null);
  let copied = $state(false);
  let copiedRow = $state<number | null>(null);
  let selectionFrame = 0;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  let rowTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    if (!scroller) return;
    const measure = () => {
      viewport = scroller?.clientHeight ?? 0;
    };
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    measure();
    document.addEventListener("selectionchange", scheduleSelection);
    return () => {
      observer.disconnect();
      document.removeEventListener("selectionchange", scheduleSelection);
      cancelAnimationFrame(selectionFrame);
      clearTimeout(copiedTimer);
      clearTimeout(rowTimer);
    };
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const end = $derived(
    Math.min(rows.length, Math.ceil((scrollTop + viewport) / ROW_HEIGHT) + OVERSCAN),
  );
  const visibleRows = $derived(rows.slice(start, end));

  const gridStyle = $derived(
    `grid-template-columns: ${GUTTER} repeat(${columns}, minmax(${COLUMN_MIN}, 1fr))`,
  );
  const contentWidth = $derived(`calc(${GUTTER} + ${columns} * ${COLUMN_MIN})`);
  const placeholders = $derived(
    Array.from({ length: Math.max(columns - (header?.length ?? 0), 0) }),
  );

  const classFor = (column: number) =>
    rainbow ? `rainbow-col-${column % RAINBOW_COLUMN_COUNT}` : "";

  function handleScroll() {
    scrollTop = scroller?.scrollTop ?? 0;
    selection = null;
  }

  function scheduleSelection() {
    if (selectionFrame) return;
    selectionFrame = requestAnimationFrame(() => {
      selectionFrame = 0;
      const active = window.getSelection();
      if (!active || active.rangeCount === 0 || active.isCollapsed) {
        selection = null;
        return;
      }
      const anchor = active.anchorNode;
      if (!root || !anchor || !root.contains(anchor)) {
        selection = null;
        return;
      }
      const rect = active.getRangeAt(0).getBoundingClientRect();
      if (!rect || (rect.width === 0 && rect.height === 0)) {
        selection = null;
        return;
      }
      const rootRect = root.getBoundingClientRect();
      selection = {
        x: rect.left + rect.width / 2 - rootRect.left,
        y: rect.bottom - rootRect.top + 8,
      };
    });
  }

  // Drops the row/column gutter and emits a rectangular, tab-separated grid, so a
  // selection pastes cleanly into a spreadsheet or another CSV.
  function selectionText(): string | null {
    const active = window.getSelection();
    if (!active || active.rangeCount === 0 || active.isCollapsed) return null;
    const range = active.getRangeAt(0);
    const cells = scroller?.querySelectorAll<HTMLElement>("[data-cell]") ?? [];
    const picked: { row: number; column: number; text: string }[] = [];
    for (const cell of cells) {
      if (range.intersectsNode(cell)) {
        picked.push({
          row: Number(cell.dataset.row),
          column: Number(cell.dataset.column),
          text: cell.textContent ?? "",
        });
      }
    }
    if (picked.length === 0) return null;
    if (picked.length === 1) return active.toString();

    const byRow = new Map<number, Map<number, string>>();
    let minColumn = Infinity;
    let maxColumn = -Infinity;
    for (const item of picked) {
      let cellsOfRow = byRow.get(item.row);
      if (!cellsOfRow) {
        cellsOfRow = new Map();
        byRow.set(item.row, cellsOfRow);
      }
      cellsOfRow.set(item.column, item.text);
      minColumn = Math.min(minColumn, item.column);
      maxColumn = Math.max(maxColumn, item.column);
    }
    const lines: string[] = [];
    for (const row of [...byRow.keys()].sort((a, b) => a - b)) {
      const cellsOfRow = byRow.get(row)!;
      const line: string[] = [];
      for (let column = minColumn; column <= maxColumn; column++) {
        line.push(cellsOfRow.get(column) ?? "");
      }
      lines.push(line.join("\t"));
    }
    return lines.join("\n");
  }

  function handleCopy(event: ClipboardEvent) {
    const text = selectionText();
    if (!text) return;
    event.clipboardData?.setData("text/plain", text);
    event.preventDefault();
  }

  function tsvField(value: string): string {
    return /[\t\n\r"]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value;
  }

  async function copyRow(rowIndex: number) {
    const row = rows[rowIndex];
    if (!row) return;
    const text = Array.from({ length: columns }, (_, column) => tsvField(row[column] ?? "")).join(
      "\t",
    );
    if (!(await writeToClipboard(text))) return;
    copiedRow = rowIndex;
    clearTimeout(rowTimer);
    rowTimer = setTimeout(() => {
      copiedRow = null;
    }, 1100);
  }

  async function copySelection() {
    const text = selectionText();
    if (!text) return;
    if (!(await writeToClipboard(text))) return;
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => {
      copied = false;
    }, 1100);
  }

  // `navigator.clipboard` needs a secure context and a recent WebKitGTK; the
  // execCommand path keeps copy working on older Linux WebViews.
  async function writeToClipboard(text: string): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      /* fall through */
    }
    try {
      const field = document.createElement("textarea");
      field.value = text;
      field.setAttribute("readonly", "");
      field.style.position = "fixed";
      field.style.opacity = "0";
      document.body.append(field);
      field.select();
      const ok = document.execCommand("copy");
      field.remove();
      return ok;
    } catch {
      return false;
    }
  }
</script>

<div bind:this={root} class="relative h-full min-h-0">
  {#if rows.length === 0 && !hasHeader}
    <div class="grid h-full place-items-center text-[13px] text-[var(--app-fg-muted)]">{emptyLabel}</div>
  {:else}
    <div class="flex h-full min-h-0 flex-col">
      {#if notice}
        <p class="shrink-0 border-b border-[var(--app-border)] px-3 py-1 text-[11px] text-[var(--app-fg-muted)]">
          {notice}
        </p>
      {/if}
      <div
        bind:this={scroller}
        onscroll={handleScroll}
        oncopy={handleCopy}
        class="min-h-0 flex-1 overflow-auto">
        <div class="min-h-full" style="min-width: {contentWidth}">
          {#if hasHeader}
            <div class="grid-head sticky top-0 z-20 grid bg-[var(--app-surface)]" style={gridStyle}>
              <div
                class="sticky left-0 z-30 bg-[var(--app-surface)] px-3 py-1.5 text-[11px] font-medium text-[var(--app-fg-muted)]"
                >#</div>
              {#each header ?? [] as cell, column}
                <div
                  data-cell
                  data-row="0"
                  data-column={column}
                  class="{classFor(column)} overflow-hidden text-ellipsis whitespace-nowrap px-3 py-1.5 text-[12px] font-semibold"
                  title={cell}>{cell}</div>
              {/each}
              {#each placeholders as _}
                <div class="px-3 py-1.5"></div>
              {/each}
            </div>
          {/if}
          <div class="relative" style="height: {rows.length * ROW_HEIGHT}px">
            <div class="absolute inset-x-0 top-0" style="transform: translateY({start * ROW_HEIGHT}px)">
              {#each visibleRows as row, index (start + index)}
                {@const rowIndex = start + index}
                <div class="grid-row grid" style={gridStyle}>
                  <div
                    class="grid-gutter sticky left-0 z-10 bg-[var(--app-input)] px-2 text-right text-[11px] text-[var(--app-fg-faint)]">
                    <span class="grid-gutter-number">{firstRowNumber + rowIndex}</span>
                    <button
                      type="button"
                      class="grid-row-copy size-5 place-items-center rounded border-0 bg-transparent text-[var(--app-fg-muted)] hover:bg-[var(--app-surface-raised)] hover:text-[var(--app-fg)]"
                      aria-label="Copy row {firstRowNumber + rowIndex}"
                      title="Copy row"
                      onclick={() => void copyRow(rowIndex)}>
                      {#if copiedRow === rowIndex}
                        <CheckIcon class="size-3.5 text-emerald-400" />
                      {:else}
                        <CopyIcon class="size-3.5" />
                      {/if}
                    </button>
                  </div>
                  {#each Array.from({ length: columns }) as _, column}
                    {@const value = row[column] ?? ""}
                    <div
                      data-cell
                      data-row={rowIndex + 1}
                      data-column={column}
                      class="{classFor(column)} overflow-hidden text-ellipsis whitespace-nowrap px-3 leading-[30px] text-[12.5px]"
                      title={value}>{value}</div>
                  {/each}
                </div>
              {/each}
            </div>
          </div>
        </div>
      </div>
    </div>
  {/if}

  {#if selection}
    <button
      type="button"
      class="absolute z-30 flex -translate-x-1/2 items-center gap-1.5 whitespace-nowrap rounded-full bg-[var(--app-surface-raised)] px-3 py-1.5 text-[11.5px] font-medium text-[var(--app-fg)] shadow-[var(--app-shadow)] transition-[background-color,transform] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-[var(--app-surface)] active:scale-[0.96]"
      style="left: {selection.x}px; top: {selection.y}px;"
      onmousedown={(event) => event.preventDefault()}
      onclick={() => void copySelection()}>
      {#if copied}
        <CheckIcon class="size-3.5" />
        <span>Copied</span>
      {:else}
        <CopyIcon class="size-3.5" />
        <span>Copy</span>
      {/if}
    </button>
  {/if}
</div>

<style>
  .grid-head {
    box-shadow: inset 0 -1px 0 var(--app-border);
  }
  .grid-row {
    /* An inset line instead of a border keeps rows exactly ROW_HEIGHT tall, which
       the windowing math relies on. */
    box-shadow: inset 0 -1px 0 var(--app-border);
  }
  .grid-row:hover :global(div:not(.sticky)) {
    background-color: rgb(255 255 255 / 0.035);
  }
  .grid-gutter {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    height: 30px;
  }
  .grid-gutter-number {
    line-height: 30px;
  }
  .grid-row-copy {
    display: none;
  }
  .grid-row:hover .grid-gutter-number {
    display: none;
  }
  .grid-row:hover .grid-row-copy {
    display: grid;
  }
</style>
