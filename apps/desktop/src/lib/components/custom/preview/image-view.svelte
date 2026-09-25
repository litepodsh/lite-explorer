<script lang="ts">
  import DOMPurify from "dompurify";
  import MaximizeIcon from "@lucide/svelte/icons/maximize-2";

  type Props = {
    src: string;
    name: string;
    /** Shows the “open in a separate window” button when provided. */
    onMaximize?: () => void;
  };
  let { src, name, onMaximize }: Props = $props();

  const isSvg = $derived(/\.svg$/i.test(name));
  // SVG is inlined as live vector geometry so WebKit re-rasterizes crisply on
  // CSS transform zoom (an <img> with an SVG src gets bitmap-scaled instead).
  let cleanSvg = $state("");

  $effect(() => {
    cleanSvg = "";
    if (!isSvg) return;
    let cancelled = false;
    void fetch(src)
      .then((response) => response.text())
      .then((text) => {
        if (!cancelled) {
          cleanSvg = DOMPurify.sanitize(text, { USE_PROFILES: { svg: true, svgFilters: true } });
        }
      })
      .catch(() => {
        if (!cancelled) cleanSvg = "";
      });
    return () => {
      cancelled = true;
    };
  });

  const MIN_SCALE = 0.1;
  const MAX_SCALE = 8;
  const BUTTON_STEP = 0.25;
  const WHEEL_SENSITIVITY = 0.0018;
  const PINCH_SENSITIVITY = 0.01;
  const EASE = 0.22;

  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let container = $state<HTMLElement | null>(null);
  let dragging = $state(false);
  let dragStart = { x: 0, y: 0, tx: 0, ty: 0 };

  let dispScale = $state(1);
  let dispTx = $state(0);
  let dispTy = $state(0);
  let raf = 0;

  const zoomLabel = $derived(`${Math.round(scale * 100)}%`);

  function clamp(value: number): number {
    return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));
  }

  function animate() {
    const ds = scale - dispScale;
    const dx = tx - dispTx;
    const dy = ty - dispTy;
    if (Math.abs(ds) < 0.0005 && Math.abs(dx) < 0.05 && Math.abs(dy) < 0.05) {
      dispScale = scale;
      dispTx = tx;
      dispTy = ty;
      raf = 0;
      return;
    }
    dispScale += ds * EASE;
    dispTx += dx * EASE;
    dispTy += dy * EASE;
    raf = requestAnimationFrame(animate);
  }

  function ensureAnimating() {
    if (!raf) raf = requestAnimationFrame(animate);
  }

  function zoomAt(nextScale: number, cx: number, cy: number) {
    const clamped = clamp(nextScale);
    tx = cx - (clamped / scale) * (cx - tx);
    ty = cy - (clamped / scale) * (cy - ty);
    scale = clamped;
    ensureAnimating();
  }

  function handleWheel(event: WheelEvent) {
    event.preventDefault();
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const cx = event.clientX - rect.left - rect.width / 2;
    const cy = event.clientY - rect.top - rect.height / 2;
    const sensitivity = event.ctrlKey ? PINCH_SENSITIVITY : WHEEL_SENSITIVITY;
    const factor = Math.exp(-event.deltaY * sensitivity);
    zoomAt(scale * factor, cx, cy);
  }

  function zoomBy(factor: number) {
    zoomAt(scale * factor, 0, 0);
  }

  function reset() {
    scale = 1;
    tx = 0;
    ty = 0;
    ensureAnimating();
  }

  function handlePointerDown(event: PointerEvent) {
    dragging = true;
    dragStart = { x: event.clientX, y: event.clientY, tx, ty };
    (event.target as HTMLElement).setPointerCapture(event.pointerId);
  }

  function handlePointerMove(event: PointerEvent) {
    if (!dragging) return;
    tx = dragStart.tx + (event.clientX - dragStart.x);
    ty = dragStart.ty + (event.clientY - dragStart.y);
    dispTx = tx;
    dispTy = ty;
  }

  function handlePointerUp(event: PointerEvent) {
    dragging = false;
    try {
      (event.target as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      /* pointer already released */
    }
  }

  function handleDoubleClick() {
    reset();
  }
</script>

<div
  bind:this={container}
  class="relative grid h-full w-full place-items-center overflow-hidden p-3 {dragging
    ? 'cursor-grabbing'
    : 'cursor-grab'}"
  role="group"
  aria-label="Image preview"
  onwheel={handleWheel}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
  ondblclick={handleDoubleClick}
>
  <div
    class="grid h-full w-full place-items-center"
    style="transform: translate({dispTx}px, {dispTy}px); will-change: transform;"
  >
    {#if isSvg && cleanSvg}
      <div
        class="svg-preview select-none"
        role="img"
        aria-label={name}
        style="width: {dispScale * 100}%; height: {dispScale * 100}%;"
      >
        {@html cleanSvg}
      </div>
    {:else}
      <img
        {src}
        alt={name}
        class="max-h-full max-w-full select-none rounded-md"
        style="transform: scale({dispScale}); transform-origin: center center; will-change: transform;"
        draggable="false"
      />
    {/if}
  </div>

  <div
    class="absolute right-2 bottom-2 z-10 flex items-center gap-1 rounded-md border border-[#3a3734] bg-[#2d2a28] p-0.5 text-[11px]"
    role="group"
    aria-label="Zoom controls"
  >
    <button
      class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836]"
      aria-label="Zoom out"
      onclick={() => zoomBy(1 - BUTTON_STEP)}>−</button>
    <span class="min-w-12 select-none text-center text-[#9c9895]" aria-live="polite">Zoom {zoomLabel}</span>
    <button
      class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836]"
      aria-label="Zoom in"
      onclick={() => zoomBy(1 + BUTTON_STEP)}>+</button>
    {#if onMaximize}
      <span class="mx-0.5 h-4 w-px bg-[#3a3734]"></span>
      <button
        class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836]"
        aria-label="Open in a separate window"
        title="Open in a separate window"
        onclick={onMaximize}><MaximizeIcon class="size-3.5" /></button>
    {/if}
  </div>
</div>

<style>
  .svg-preview :global(svg) {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
