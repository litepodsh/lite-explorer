<script lang="ts">
  import { onMount } from "svelte";
  import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

  type Props = {
    doc: PDFDocumentProxy;
    index: number;
    baseWidth: number;
    baseHeight: number;
    width: number;
    active: boolean;
    scrollRoot: HTMLElement | null;
    onSelect: (index: number) => void;
  };
  let { doc, index, baseWidth, baseHeight, width, active, scrollRoot, onSelect }: Props = $props();

  const height = $derived((baseHeight / baseWidth) * width);

  let root = $state<HTMLElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let painted = $state(false);
  let task: RenderTask | null = null;

  onMount(() => {
    if (!root) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) void paint();
      },
      { root: scrollRoot, rootMargin: "300px 0px" },
    );
    observer.observe(root);
    return () => {
      observer.disconnect();
      task?.cancel();
      task = null;
    };
  });

  async function paint() {
    if (painted) return;
    const page = await doc.getPage(index + 1);
    const target = canvas;
    if (!target) return;
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const viewport = page.getViewport({ scale: width / baseWidth });
    target.width = Math.floor(viewport.width * ratio);
    target.height = Math.floor(viewport.height * ratio);
    target.style.width = `${viewport.width}px`;
    target.style.height = `${viewport.height}px`;
    const rendering = page.render({
      canvas: target,
      viewport,
      transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
    });
    task = rendering;
    try {
      await rendering.promise;
      task = null;
      painted = true;
    } catch {
      /* cancelled by the observer when scrolled away */
    }
  }
</script>

<button
  bind:this={root}
  type="button"
  class="group flex shrink-0 flex-col items-center gap-1.5 rounded-xl border-0 bg-transparent p-1 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.97]"
  aria-label={`Go to page ${index + 1}`}
  aria-current={active ? "page" : undefined}
  onclick={() => onSelect(index)}
>
  <span
    class="block overflow-hidden rounded-md bg-white transition-shadow duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] {active
      ? 'shadow-[0_0_0_2px_#0a9bff,0_10px_24px_-12px_rgb(0_0_0/0.7)]'
      : 'shadow-[0_0_0_1px_rgb(255_255_255/0.1),0_8px_18px_-12px_rgb(0_0_0/0.7)] group-hover:shadow-[0_0_0_1px_rgb(255_255_255/0.22),0_10px_22px_-12px_rgb(0_0_0/0.7)]'}"
    style="width: {width}px; height: {height}px;"
  >
    <canvas bind:this={canvas} class="block"></canvas>
  </span>
  <span class="text-[10px] tabular-nums {active ? 'text-[#f2f1f0]' : 'text-[#8d8986]'}">{index + 1}</span>
</button>
