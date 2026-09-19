<script lang="ts">
  import { openSketch, type SketchPage, type SketchPreview } from "./sketch.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let sketch = $state.raw<SketchPreview | null>(null);
  let selected = $state(0);
  let error = $state("");
  let token = 0;

  const page = $derived<SketchPage | null>(sketch?.pages[selected] ?? null);
  const view = $derived.by(() => {
    if (!page) return null;
    const width = page.width || 400;
    const height = page.height || 300;
    const pad = Math.max(width, height) * 0.05;
    return { width, height, pad, box: `${-pad} ${-pad} ${width + pad * 2} ${height + pad * 2}` };
  });

  function fill(kind: string): string {
    if (kind === "text") return "transparent";
    if (kind === "artboard") return "rgba(122,162,247,0.08)";
    return "rgba(224,175,104,0.18)";
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    sketch = null;
    selected = 0;
    error = "";
    openSketch(target)
      .then((result) => {
        if (request === token) sketch = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="flex h-full min-h-0 flex-col bg-[#1f1d1b]" aria-label={`Sketch preview of ${name}`}>
  {#if error}
    <div class="grid flex-1 place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if sketch}
    <div class="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-[#3a3734] px-2 py-1.5">
      {#each sketch.pages as item, index (index)}
        <button
          type="button"
          class="shrink-0 rounded px-2 py-1 text-[11.5px] {index === selected
            ? 'bg-[#3b3836] text-[#e8e5e2]'
            : 'text-[#9c9895] hover:bg-[#2a2825]'}"
          onclick={() => (selected = index)}>
          {item.name}
        </button>
      {/each}
    </div>
    {#if page && view}
      <div class="min-h-0 flex-1 overflow-auto p-4">
        <svg
          class="mx-auto max-h-full"
          viewBox={view.box}
          style="max-width: 100%; background: #ffffff;"
          preserveAspectRatio="xMidYMid meet">
          {#each page.layers as layer, index (index)}
            <rect
              x={layer.x}
              y={layer.y}
              width={layer.width}
              height={layer.height}
              fill={fill(layer.kind)}
              stroke={layer.kind === "artboard" ? "#7aa2f7" : "#b08948"}
              stroke-width="1"
              stroke-dasharray={layer.visible ? "" : "4 3"}
              opacity={layer.visible ? 1 : 0.4} />
            {#if layer.kind === "text" && layer.text}
              <text x={layer.x} y={layer.y + Math.min(layer.height, 14)} font-size={Math.min(14, layer.height || 14)} fill="#333">
                {layer.text.slice(0, 40)}
              </text>
            {/if}
          {/each}
        </svg>
      </div>
      <p class="shrink-0 border-t border-[#3a3734] px-3 py-1 text-[11px] text-[#9c9895]">
        {page.layers.length} capas · {page.width}×{page.height}
      </p>
    {/if}
  {/if}
</div>
