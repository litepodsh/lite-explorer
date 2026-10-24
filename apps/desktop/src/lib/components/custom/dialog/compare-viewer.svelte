<script lang="ts">
  // Before/after viewer: the original on the left of the divider, the converted
  // image on the right. Zoom and pan are shared by both layers.
  import {
    IDENTITY,
    dividerAt,
    panBy,
    swapsAxes,
    zoomAt,
    type View,
  } from "#lib/file-ops/compare-view.js";

  let {
    originalSrc,
    convertedSrc,
    rotate = 0,
    flipH = false,
    flipV = false,
    originalLabel,
    convertedLabel,
    loading = false,
    onConvertedError,
  }: {
    originalSrc: string;
    convertedSrc: string;
    rotate?: number;
    flipH?: boolean;
    flipV?: boolean;
    originalLabel: string;
    convertedLabel: string;
    loading?: boolean;
    /** The converted image could not be displayed (the web view lacks its codec). */
    onConvertedError?: () => void;
  } = $props();

  let stage = $state<HTMLDivElement | null>(null);
  let view = $state<View>(IDENTITY);
  let divider = $state(50);
  let panning: { x: number; y: number } | null = null;

  const swapped = $derived(swapsAxes(rotate));
  const contentTransform = $derived(
    `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
  );
  // The original is shown as the browser decodes it; rotation and flips are applied here
  // so it matches the converted image, which already has them baked in.
  const originalStyle = $derived(
    `width: ${swapped ? "100cqh" : "100cqw"}; height: ${swapped ? "100cqw" : "100cqh"}; ` +
      `transform: translate(-50%, -50%) scaleX(${flipH ? -1 : 1}) scaleY(${flipV ? -1 : 1}) rotate(${rotate}deg);`,
  );

  // Wheel must be non-passive to stop the page from scrolling while zooming.
  $effect(() => {
    const element = stage;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      view = zoomAt(
        view,
        Math.exp(-event.deltaY * 0.0018),
        event.clientX - rect.left,
        event.clientY - rect.top,
        rect.width,
        rect.height,
      );
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  });

  function startPan(event: PointerEvent) {
    if (view.scale <= 1 || !stage) return;
    stage.setPointerCapture(event.pointerId);
    panning = { x: event.clientX, y: event.clientY };
  }

  function movePan(event: PointerEvent) {
    if (!panning || !stage) return;
    const rect = stage.getBoundingClientRect();
    view = panBy(
      view,
      event.clientX - panning.x,
      event.clientY - panning.y,
      rect.width,
      rect.height,
    );
    panning = { x: event.clientX, y: event.clientY };
  }

  function endPan() {
    panning = null;
  }

  function startDrag(event: PointerEvent) {
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function moveDrag(event: PointerEvent) {
    if (!(event.currentTarget as HTMLElement).hasPointerCapture(event.pointerId) || !stage) return;
    const rect = stage.getBoundingClientRect();
    divider = dividerAt(event.clientX, rect.left, rect.width);
  }

  function keyDrag(event: KeyboardEvent) {
    const step = event.shiftKey ? 10 : 2;
    if (event.key === "ArrowLeft") divider = Math.max(0, divider - step);
    else if (event.key === "ArrowRight") divider = Math.min(100, divider + step);
    else if (event.key === "Home") divider = 0;
    else if (event.key === "End") divider = 100;
    else return;
    event.preventDefault();
  }
</script>

<div class="viewer">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="stage"
    class:zoomed={view.scale > 1}
    bind:this={stage}
    onpointerdown={startPan}
    onpointermove={movePan}
    onpointerup={endPan}
    onpointercancel={endPan}
    ondblclick={() => (view = IDENTITY)}>
    <div class="layer">
      <div class="content" style:transform={contentTransform}>
        {#if originalSrc}
          <img class="original" src={originalSrc} alt="Original" style={originalStyle} draggable="false" />
        {/if}
      </div>
    </div>
    <div class="layer" style:clip-path={`inset(0 0 0 ${divider}%)`}>
      <div class="content" style:transform={contentTransform}>
        {#if convertedSrc}
          <img
            class="converted"
            src={convertedSrc}
            alt="Converted"
            draggable="false"
            onerror={() => onConvertedError?.()} />
        {/if}
      </div>
    </div>

    <div
      class="handle"
      style:left={`${divider}%`}
      role="slider"
      tabindex="0"
      aria-label="Compare original and converted"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(divider)}
      onpointerdown={startDrag}
      onpointermove={moveDrag}
      onkeydown={keyDrag}>
      <span class="grip"></span>
    </div>

    <span class="tag left">{originalLabel}</span>
    <span class="tag right">{convertedLabel}</span>
    {#if loading}
      <span class="tag busy" role="status">Updating…</span>
    {/if}
  </div>
  <p class="hint">Drag the divider to compare. Scroll to zoom, double-click to reset.</p>
</div>

<style>
  .viewer {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  .stage {
    position: relative;
    container-type: size;
    width: 100%;
    height: var(--compare-height, min(62vh, 460px));
    overflow: hidden;
    border-radius: calc(var(--app-radius) - 4px);
    background:
      conic-gradient(var(--app-surface-raised) 25%, var(--app-input) 0 50%, var(--app-surface-raised) 0 75%, var(--app-input) 0)
      0 0 / 16px 16px;
    box-shadow: inset 0 0 0 1px var(--app-border);
    user-select: none;
    touch-action: none;
  }
  .stage.zoomed {
    cursor: grab;
  }
  .stage.zoomed:active {
    cursor: grabbing;
  }
  .layer {
    position: absolute;
    inset: 0;
  }
  .content {
    position: absolute;
    inset: 0;
    transform-origin: 0 0;
  }
  .original {
    position: absolute;
    top: 50%;
    left: 50%;
    object-fit: contain;
    pointer-events: none;
  }
  .converted {
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 2;
    width: 24px;
    margin-left: -12px;
    cursor: ew-resize;
    outline: none;
    touch-action: none;
  }
  .handle::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 2px;
    margin-left: -1px;
    background: var(--app-accent-fg);
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.35);
  }
  .grip {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 22px;
    height: 22px;
    margin: -11px 0 0 -11px;
    border-radius: 999px;
    background: var(--app-accent);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.25),
      0 0 0 2px var(--app-accent-fg),
      0 2px 8px rgb(0 0 0 / 0.4);
  }
  .handle:focus-visible .grip {
    outline: 2px solid var(--app-accent-fg);
    outline-offset: 2px;
  }
  .tag {
    position: absolute;
    z-index: 3;
    max-width: 46%;
    padding: 3px 8px;
    overflow: hidden;
    border-radius: 999px;
    background: color-mix(in srgb, var(--app-surface) 82%, transparent);
    backdrop-filter: blur(6px);
    box-shadow: inset 0 0 0 1px var(--app-border-strong);
    color: var(--app-fg);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    text-overflow: ellipsis;
    white-space: nowrap;
    pointer-events: none;
  }
  .tag.left {
    top: 8px;
    left: 8px;
  }
  .tag.right {
    top: 8px;
    right: 8px;
  }
  .tag.busy {
    bottom: 8px;
    left: 50%;
    translate: -50% 0;
    color: var(--app-fg-muted);
  }
  .hint {
    margin: 0;
    color: var(--app-fg-faint);
    font-size: 11px;
  }
</style>
