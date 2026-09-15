<script lang="ts">
  import { onMount } from "svelte";
  import type { PDFDocumentProxy, PDFPageProxy, RenderTask } from "pdfjs-dist";
  import { loadPdfjs } from "./pdf.js";

  type Props = {
    doc: PDFDocumentProxy;
    index: number;
    baseWidth: number;
    baseHeight: number;
    scale: number;
    scrollRoot: HTMLElement | null;
  };
  let { doc, index, baseWidth, baseHeight, scale, scrollRoot }: Props = $props();

  const width = $derived(baseWidth * scale);
  const height = $derived(baseHeight * scale);

  let root = $state<HTMLElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let textLayer = $state<HTMLElement | null>(null);
  let painted = $state(false);
  let visible = $state(false);

  let version = 0;
  let task: RenderTask | null = null;
  let textBuilt = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Measuring helpers, shared across pages.
  let measureCanvas: HTMLCanvasElement | null = null;
  let measureCtx: CanvasRenderingContext2D | null = null;
  const ascentCache = new Map<string, number>();

  onMount(() => {
    if (!root) return;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          visible = entry.isIntersecting;
          if (entry.isIntersecting) schedule(0);
        }
      },
      { root: scrollRoot, rootMargin: "700px 0px" },
    );
    observer.observe(root);
    return () => {
      observer.disconnect();
      clearTimeout(timer);
      task?.cancel();
      task = null;
    };
  });

  // Canvas is re-rasterized only after the size settles, so dragging the panel
  // stretches the existing bitmap instead of repainting every frame.
  $effect(() => {
    void scale;
    void doc;
    if (visible) schedule(140);
  });

  function schedule(delay: number) {
    clearTimeout(timer);
    timer = setTimeout(() => void paint(), delay);
  }

  async function paint() {
    const page = await doc.getPage(index + 1);
    const current = ++version;
    task?.cancel();
    task = null;

    const viewport = page.getViewport({ scale });
    const target = canvas;
    if (!target) return;
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    target.width = Math.floor(viewport.width * ratio);
    target.height = Math.floor(viewport.height * ratio);

    const rendering = page.render({
      canvas: target,
      viewport,
      transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
    });
    task = rendering;
    try {
      await rendering.promise;
    } catch (error) {
      if ((error as { name?: string })?.name === "RenderingCancelledException") return;
      throw error;
    }
    if (current !== version) return;
    task = null;
    painted = true;
    if (!textBuilt) {
      textBuilt = true;
      await buildText(page, viewport).catch(() => (textBuilt = false));
    }
  }

  function measurementContext(): CanvasRenderingContext2D | null {
    if (!measureCanvas) measureCanvas = document.createElement("canvas");
    if (!measureCtx) measureCtx = measureCanvas.getContext("2d");
    return measureCtx;
  }

  function genericFamily(value: string | undefined): string {
    return value === "sans-serif" || value === "serif" || value === "monospace" ? value : "sans-serif";
  }

  function ascentRatio(family: string, style: { ascent?: number; descent?: number } | undefined): number {
    const cached = ascentCache.get(family);
    if (cached) return cached;
    let ratio = 0.8;
    const ctx = measurementContext();
    if (ctx) {
      ctx.canvas.width = ctx.canvas.height = 64;
      ctx.font = `64px ${family}`;
      const metrics = ctx.measureText("");
      const ascent = metrics.fontBoundingBoxAscent;
      const descent = Math.abs(metrics.fontBoundingBoxDescent);
      if (ascent) ratio = ascent / (ascent + descent);
      else if (style?.ascent) ratio = style.ascent;
      else if (style?.descent) ratio = 1 + style.descent;
      ctx.canvas.width = ctx.canvas.height = 0;
    }
    ascentCache.set(family, ratio);
    return ratio;
  }

  function measureWidth(family: string, fontSize: number, text: string): number {
    const ctx = measurementContext();
    if (!ctx) return 0;
    ctx.font = `${fontSize}px ${family}`;
    return ctx.measureText(text).width;
  }

  // A selectable text layer built by hand. It works on every WebView, unlike
  // pdf.js' TextLayer, which needs Promise.withResolvers and other new APIs.
  async function buildText(page: PDFPageProxy, viewport: ReturnType<PDFPageProxy["getViewport"]>) {
    const container = textLayer;
    if (!container) return;
    const pdfjs = await loadPdfjs();
    const content = await page.getTextContent();
    const styles = content.styles as Record<string, { fontFamily?: string; ascent?: number; descent?: number; vertical?: boolean }>;
    const { pageHeight, pageX, pageY } = viewport.rawDims as {
      pageWidth: number;
      pageHeight: number;
      pageX: number;
      pageY: number;
    };
    const transform = [1, 0, 0, -1, -pageX, pageY + pageHeight];
    const fragment = document.createDocumentFragment();

    for (const item of content.items) {
      if (!("str" in item) || !item.str) continue;
      const style = styles[item.fontName];
      const family = genericFamily(style?.fontFamily);
      const tx = pdfjs.Util.transform(transform, item.transform);
      let angle = Math.atan2(tx[1], tx[0]);
      if (style?.vertical) angle += Math.PI / 2;
      const fontHeight = Math.hypot(tx[2], tx[3]);
      const fontAscent = fontHeight * ascentRatio(family, style);
      const left = angle === 0 ? tx[4] : tx[4] + fontAscent * Math.sin(angle);
      const top = angle === 0 ? tx[5] - fontAscent : tx[5] - fontAscent * Math.cos(angle);

      const span = document.createElement("span");
      span.textContent = item.str;
      span.style.left = `${left.toFixed(2)}px`;
      span.style.top = `${top.toFixed(2)}px`;
      span.style.fontSize = `${fontHeight.toFixed(2)}px`;
      span.style.fontFamily = family;
      if (item.str.length > 1 && item.width) {
        const measured = measureWidth(family, fontHeight, item.str);
        if (measured > 0) span.style.setProperty("--scale-x", (item.width / measured).toFixed(3));
      }
      const rotate = angle ? `rotate(${angle}rad) ` : "";
      span.style.transform = `${rotate}scaleX(var(--scale-x, 1))`;
      fragment.append(span);
    }
    container.replaceChildren(fragment);
  }
</script>

<div
  bind:this={root}
  data-page={index}
  data-index={index}
  class="pdf-page relative shrink-0 rounded-[13px] bg-[linear-gradient(180deg,rgb(255_255_255/0.04),rgb(255_255_255/0.01))] p-[3px] shadow-[inset_0_0_0_1px_rgb(255_255_255/0.06),0_26px_50px_-30px_rgb(0_0_0/0.85)] transition-[opacity,transform] duration-500 ease-[cubic-bezier(0.32,0.72,0,1)] {painted
    ? 'translate-y-0 opacity-100'
    : 'translate-y-3 opacity-0'}"
>
  <div
    class="relative overflow-hidden rounded-[10px] bg-white shadow-[inset_0_1px_0_rgb(255_255_255/0.6)]"
    style="width: {width}px; height: {height}px;"
  >
    <canvas bind:this={canvas} class="block" style="width: {width}px; height: {height}px;" aria-label={`Page ${index + 1}`}></canvas>
    <div
      bind:this={textLayer}
      class="textLayer"
      style="width: {baseWidth}px; height: {baseHeight}px; transform: scale({scale});"
    ></div>
  </div>
</div>

<style>
  :global(.textLayer) {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 1;
    overflow: hidden;
    line-height: 1;
    text-align: initial;
    letter-spacing: normal;
    word-spacing: normal;
    text-size-adjust: none;
    forced-color-adjust: none;
    transform-origin: 0 0;
    caret-color: CanvasText;
  }
  :global(.textLayer span) {
    position: absolute;
    color: transparent;
    white-space: pre;
    cursor: text;
    transform-origin: 0% 0%;
    user-select: text;
    -webkit-user-select: text;
  }
  :global(.textLayer ::selection) {
    background: rgb(10 155 255 / 0.35);
  }
  @media (prefers-reduced-motion: reduce) {
    .pdf-page {
      transition: none;
    }
  }
</style>
