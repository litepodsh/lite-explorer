<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import MinusIcon from "@lucide/svelte/icons/minus";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import PanelLeftIcon from "@lucide/svelte/icons/panel-left";
  import MoveHorizontalIcon from "@lucide/svelte/icons/move-horizontal";
  import FileIcon from "@lucide/svelte/icons/file";
  import LockIcon from "@lucide/svelte/icons/lock";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import CheckIcon from "@lucide/svelte/icons/check";
  import FindBar from "./find-bar.svelte";
  import PdfPage from "./pdf-page.svelte";
  import PdfThumb from "./pdf-thumb.svelte";
  import { loadPdfjs, openPdf, type PdfLoadingTask } from "./pdf.js";

  type Props = { src: string; name: string };
  let { src, name }: Props = $props();

  type PageMeta = { width: number; height: number };

  const ZOOM_STEPS = [0.5, 0.67, 0.8, 1, 1.25, 1.5, 2, 3, 4];
  const INCORRECT_PASSWORD = 2;
  const THUMB_WIDTH = 104;

  let status = $state<"loading" | "ready" | "error" | "password">("loading");
  let errorMessage = $state("");
  let badPassword = $state(false);
  let pdf = $state.raw<PDFDocumentProxy | null>(null);
  let numPages = $state(0);
  let pages = $state.raw<PageMeta[]>([]);
  let currentPage = $state(1);
  let zoom = $state(1);
  let fitWidth = $state(true);
  let thumbsOpen = $state(false);
  let findOpen = $state(false);
  let containerWidth = $state(0);
  let password = $state("");

  let scrollEl = $state<HTMLElement | null>(null);
  let pagesEl = $state<HTMLElement | null>(null);
  let railEl = $state<HTMLElement | null>(null);
  let stageEl = $state<HTMLElement | null>(null);
  let findBar = $state<ReturnType<typeof FindBar> | null>(null);
  let passwordInput = $state<HTMLInputElement | null>(null);
  let selection = $state<{ x: number; y: number } | null>(null);
  let copied = $state(false);

  let activeTask: PdfLoadingTask | null = null;
  let pendingPassword: ((password: string) => void) | null = null;
  let aborting = false;
  let frame = 0;
  let selectionFrame = 0;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const reducedMotion = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const maxBaseWidth = $derived(pages.length ? Math.max(...pages.map((page) => page.width)) : 1);
  const fitScale = $derived(containerWidth > 0 ? Math.max(0.1, (containerWidth - 96) / maxBaseWidth) : 1);
  const scale = $derived(fitWidth ? fitScale : zoom);
  const zoomPercent = $derived(Math.round(scale * 100));

  onMount(() => {
    document.addEventListener("selectionchange", handleSelectionChange);
    return () => {
      document.removeEventListener("selectionchange", handleSelectionChange);
      cancelAnimationFrame(selectionFrame);
      clearTimeout(copiedTimer);
    };
  });

  function handleSelectionChange() {
    if (selectionFrame) return;
    selectionFrame = requestAnimationFrame(() => {
      selectionFrame = 0;
      updateSelection();
    });
  }

  function updateSelection() {
    const container = pagesEl;
    const stage = stageEl;
    const selected = window.getSelection();
    if (!container || !stage || !selected || selected.isCollapsed || selected.rangeCount === 0) {
      selection = null;
      return;
    }
    const range = selected.getRangeAt(0);
    if (!container.contains(range.commonAncestorContainer) || !selected.toString().trim()) {
      selection = null;
      return;
    }
    const rect = range.getBoundingClientRect();
    const stageRect = stage.getBoundingClientRect();
    const x = rect.left - stageRect.left + rect.width / 2;
    const y = rect.top - stageRect.top - 44;
    selection = {
      x: Math.min(Math.max(x, 52), Math.max(52, stageRect.width - 52)),
      y: Math.max(8, y),
    };
  }

  async function copySelection() {
    const text = window.getSelection()?.toString() ?? "";
    if (!text) return;
    if (!(await writeToClipboard(text))) return;
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => {
      copied = false;
      selection = null;
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

  $effect(() => {
    const dataUrl = src;
    if (!dataUrl) return;
    aborting = false;
    pendingPassword = null;
    status = "loading";
    errorMessage = "";
    badPassword = false;
    pdf = null;
    pages = [];
    numPages = 0;
    currentPage = 1;
    findOpen = false;

    let cancelled = false;
    void (async () => {
      try {
        await loadPdfjs();
        const handle = await openPdf(dataUrl, {
          onTask: (task) => (activeTask = task),
          onPassword: (update, reason) => {
            if (cancelled) return;
            pendingPassword = update;
            badPassword = reason === INCORRECT_PASSWORD;
            status = "password";
            void tick().then(() => passwordInput?.focus());
          },
        });
        if (cancelled) {
          handle.task.destroy();
          return;
        }
        const document = handle.document;
        const metas: PageMeta[] = [];
        for (let index = 1; index <= document.numPages; index++) {
          const viewport = (await document.getPage(index)).getViewport({ scale: 1 });
          metas.push({ width: viewport.width, height: viewport.height });
        }
        if (cancelled) return;
        pdf = document;
        numPages = document.numPages;
        pages = metas;
        status = "ready";
        await tick();
        measure();
        updateCurrentPage();
      } catch (error) {
        if (cancelled || aborting) return;
        if ((error as { name?: string })?.name === "PasswordException") return;
        status = "error";
        errorMessage = error instanceof Error ? error.message : String(error);
      }
    })();

    return () => {
      cancelled = true;
      activeTask?.destroy();
      activeTask = null;
      pdf = null;
    };
  });

  $effect(() => {
    const element = scrollEl;
    if (!element) return;
    const observer = new ResizeObserver(() => measure());
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  });

  function measure() {
    if (scrollEl) containerWidth = scrollEl.clientWidth;
  }

  function submitPassword(event: SubmitEvent) {
    event.preventDefault();
    const update = pendingPassword;
    if (!update) return;
    pendingPassword = null;
    status = "loading";
    update(password);
    password = "";
  }

  function cancelPassword() {
    aborting = true;
    status = "error";
    errorMessage = "This PDF is password-protected.";
    pendingPassword = null;
    activeTask?.destroy();
  }

  function handleScroll() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (selection) selection = null;
      updateCurrentPage();
    });
  }  function updateCurrentPage() {
    const container = pagesEl;
    if (!container) return;
    const top = (scrollEl?.scrollTop ?? 0) + 48;
    let found = 0;
    for (const element of container.querySelectorAll<HTMLElement>("[data-page]")) {
      if (element.offsetTop <= top) found = Number(element.dataset.index);
      else break;
    }
    currentPage = found + 1;
  }

  function goToPage(index: number) {
    const target = pagesEl?.querySelector<HTMLElement>(`[data-page="${index}"]`);
    if (!target || !scrollEl) return;
    scrollEl.scrollTo({ top: Math.max(0, target.offsetTop - 16), behavior: reducedMotion ? "auto" : "smooth" });
  }

  function commitPageInput(event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (!Number.isFinite(value)) return;
    goToPage(Math.min(numPages, Math.max(1, Math.round(value))) - 1);
  }

  function stepZoom(direction: 1 | -1) {
    const current = scale;
    const next = direction === 1
      ? ZOOM_STEPS.find((step) => step > current + 0.001) ?? ZOOM_STEPS[ZOOM_STEPS.length - 1]
      : [...ZOOM_STEPS].reverse().find((step) => step < current - 0.001) ?? ZOOM_STEPS[0];
    fitWidth = false;
    zoom = next;
  }

  function toggleFitWidth() {
    if (fitWidth) {
      zoom = fitScale;
      fitWidth = false;
    } else {
      fitWidth = true;
    }
  }

  function isActive(): boolean {
    if (findOpen) return true;
    const element = scrollEl;
    if (!element) return false;
    if (element.contains(document.activeElement)) return true;
    return element.matches(":hover");
  }

  /** Opens the find bar when this viewer is the one being used. Returns whether it handled it. */
  export function openFind(): boolean {
    if (status !== "ready" || !pdf || !isActive()) return false;
    findOpen = true;
    void tick().then(() => findBar?.focus());
    return true;
  }
</script>

<div class="relative flex h-full min-h-0 w-full bg-[#1f1d1b]" aria-label={`PDF preview of ${name}`}>
  {#if thumbsOpen && status === "ready" && pdf}
    <div
      bind:this={railEl}
      class="flex w-[148px] shrink-0 flex-col items-center gap-3 overflow-y-auto border-r border-[#332f2c] bg-[#1b1918] px-3 py-4"
      aria-label="Pages"
    >
      {#key pdf}
        {#each pages as page, index (index)}
          <PdfThumb
            doc={pdf}
            {index}
            baseWidth={page.width}
            baseHeight={page.height}
            width={THUMB_WIDTH}
            active={currentPage === index + 1}
            scrollRoot={railEl}
            onSelect={goToPage} />
        {/each}
      {/key}
    </div>
  {/if}

  <div bind:this={stageEl} class="relative flex min-h-0 min-w-0 flex-1 flex-col">
    <div
      bind:this={scrollEl}
      class="pdf-scroll min-h-0 flex-1 overflow-y-auto overscroll-contain"
      onscroll={handleScroll}
    >
      {#if status === "loading"}
        <div class="flex flex-col items-center gap-7 px-6 py-8">
          {#each [0, 1] as skeleton (skeleton)}
            <div class="w-full max-w-[560px] rounded-[13px] bg-[linear-gradient(180deg,rgb(255_255_255/0.04),rgb(255_255_255/0.01))] p-[3px] shadow-[inset_0_0_0_1px_rgb(255_255_255/0.06)]">
              <div class="h-[740px] w-full animate-pulse rounded-[10px] bg-[#262422]"></div>
            </div>
          {/each}
        </div>
      {:else if status === "password"}
        <div class="grid h-full place-items-center px-6">
          <div
            class="w-full max-w-[300px] rounded-[18px] bg-[linear-gradient(180deg,rgb(255_255_255/0.05),rgb(255_255_255/0.02))] p-[3px] shadow-[inset_0_0_0_1px_rgb(255_255_255/0.08),0_30px_60px_-30px_rgb(0_0_0/0.9)]"
          >
            <form
              class="flex flex-col items-center gap-3 rounded-[15px] bg-[#211f1d] px-6 py-7 text-center shadow-[inset_0_1px_0_rgb(255_255_255/0.05)]"
              onsubmit={submitPassword}
            >
              <span class="grid size-11 place-items-center rounded-full bg-white/[0.06] text-[#d9d6d3] shadow-[inset_0_1px_0_rgb(255_255_255/0.1)]">
                <LockIcon class="size-5 stroke-[1.6]" />
              </span>
              <p class="text-[14px] font-semibold text-[#e8e5e2]">Protected PDF</p>
              <p class="text-[12px] text-[#9c9895]">{badPassword ? "Wrong password, try again." : "Enter the password to open this document."}</p>
              <input
                bind:this={passwordInput}
                bind:value={password}
                type="password"
                placeholder="Password"
                autocomplete="off"
                class="h-9 w-full rounded-[10px] border border-white/10 bg-black/30 px-3 text-[13px] text-[#f2f1f0] outline-none transition-[border-color,box-shadow] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] placeholder:text-[#7c7874] focus:border-[#0a9bff]/60 focus:shadow-[0_0_0_3px_rgb(10_155_255/0.15)]" />
              <div class="mt-1 flex w-full items-center gap-2">
                <button
                  type="button"
                  class="h-9 flex-1 rounded-full border-0 bg-white/[0.05] text-[12.5px] font-medium text-[#c9c4c0] transition-[background-color,transform] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/[0.09] active:scale-[0.97]"
                  onclick={cancelPassword}>Cancel</button>
                <button
                  type="submit"
                  class="h-9 flex-1 rounded-full border-0 bg-[#0a9bff] text-[12.5px] font-semibold text-[#04121f] shadow-[inset_0_1px_0_rgb(255_255_255/0.35)] transition-[filter,transform] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:brightness-110 active:scale-[0.97]"
                  >Unlock</button>
              </div>
            </form>
          </div>
        </div>
      {:else if status === "error"}
        <div class="grid h-full place-content-center justify-items-center gap-1 px-6 text-center">
          <FileIcon class="mb-2 size-12 stroke-[1.2] text-[#67635f]" />
          <p class="text-[14px] font-semibold text-[#e8e5e2]">Can’t preview this PDF</p>
          <p class="text-[12px] break-all text-[#9c9895]">{errorMessage}</p>
        </div>
      {:else}
        <div bind:this={pagesEl} class="flex flex-col items-center gap-7 px-6 pt-8 pb-28">
          {#if pdf}
            {#key pdf}
              {#each pages as page, index (index)}
                <PdfPage doc={pdf} {index} baseWidth={page.width} baseHeight={page.height} {scale} scrollRoot={scrollEl} />
              {/each}
            {/key}
          {/if}
        </div>
      {/if}
    </div>

    {#if status === "ready"}
      <div
        class="pdf-toolbar absolute bottom-4 left-1/2 z-20 flex max-w-[calc(100%-1rem)] -translate-x-1/2 items-center gap-0.5 overflow-x-auto whitespace-nowrap rounded-full px-1.5 py-1"
        role="toolbar"
        aria-label="PDF controls"
      >
        <button
          type="button"
          class="pdf-tool {thumbsOpen ? 'bg-white/[0.12] text-[#f2f1f0]' : ''}"
          aria-label="Toggle page thumbnails"
          aria-pressed={thumbsOpen}
          onclick={() => (thumbsOpen = !thumbsOpen)}><PanelLeftIcon class="size-[15px] stroke-[1.7]" /></button
        >
        <span class="mx-0.5 h-4 w-px shrink-0 bg-white/10"></span>
        <button type="button" class="pdf-tool" aria-label="Previous page" disabled={currentPage <= 1} onclick={() => goToPage(currentPage - 2)}
          ><ChevronLeftIcon class="size-[15px] stroke-[1.7]" /></button
        >
        <div class="flex shrink-0 items-center gap-1 whitespace-nowrap px-1 text-[11px] tabular-nums text-[#c9c4c0]">
          <input
            value={currentPage}
            onchange={commitPageInput}
            aria-label="Page number"
            class="h-6 w-6 shrink-0 rounded-md border-0 bg-white/[0.05] text-center text-[11px] text-[#f2f1f0] outline-none transition-shadow duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] focus:shadow-[0_0_0_3px_rgb(10_155_255/0.18)]" />
          <span class="shrink-0 text-[10.5px] text-[#8d8986]">/ {numPages}</span>
        </div>
        <button type="button" class="pdf-tool" aria-label="Next page" disabled={currentPage >= numPages} onclick={() => goToPage(currentPage)}
          ><ChevronRightIcon class="size-[15px] stroke-[1.7]" /></button
        >
        <span class="mx-0.5 h-4 w-px shrink-0 bg-white/10"></span>
        <button type="button" class="pdf-tool" aria-label="Zoom out" onclick={() => stepZoom(-1)}><MinusIcon class="size-[15px] stroke-[1.7]" /></button
        >
        <button
          type="button"
          class="min-w-11 rounded-full border-0 bg-transparent px-1 text-center text-[11.5px] tabular-nums text-[#c9c4c0] transition-colors duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-white/[0.08] hover:text-[#f2f1f0]"
          aria-label="Fit width"
          title="Fit width"
          onclick={toggleFitWidth}>{zoomPercent}%</button
        >
        <button type="button" class="pdf-tool" aria-label="Zoom in" onclick={() => stepZoom(1)}><PlusIcon class="size-[15px] stroke-[1.7]" /></button
        >
        <button
          type="button"
          class="pdf-tool {fitWidth ? 'bg-white/[0.12] text-[#f2f1f0]' : ''}"
          aria-label="Fit width"
          aria-pressed={fitWidth}
          onclick={toggleFitWidth}><MoveHorizontalIcon class="size-[15px] stroke-[1.7]" /></button
        >
      </div>
    {/if}

    {#if findOpen && pagesEl}
      <FindBar bind:this={findBar} root={pagesEl} onClose={() => (findOpen = false)} />
    {/if}

    {#if selection}
      <button
        type="button"
        class="absolute z-30 flex -translate-x-1/2 items-center gap-1.5 whitespace-nowrap rounded-full bg-[rgb(48_45_43/0.96)] px-3 py-1.5 text-[11.5px] font-medium text-[#f2f1f0] shadow-[inset_0_0_0_1px_rgb(255_255_255/0.1),0_14px_30px_-12px_rgb(0_0_0/0.8)] transition-[background-color,transform] duration-200 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-[rgb(62_58_55/0.98)] active:scale-[0.96]"
        style="left: {selection.x}px; top: {selection.y}px;"
        aria-label="Copy selected text"
        onmousedown={(event) => event.preventDefault()}
        onclick={copySelection}
      >
        {#if copied}
          <CheckIcon class="size-3.5 stroke-[2] text-emerald-400" /> Copied
        {:else}
          <CopyIcon class="size-3.5 stroke-[1.8]" /> Copy
        {/if}
      </button>
    {/if}
  </div>
</div>

<style>
  .pdf-toolbar {
    background: rgb(45 42 40 / 0.82);
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.09),
      inset 0 1px 0 rgb(255 255 255 / 0.08),
      0 16px 40px -16px rgb(0 0 0 / 0.7);
    -webkit-backdrop-filter: blur(24px);
    backdrop-filter: blur(24px);
  }
  @media (prefers-reduced-transparency: reduce) {
    .pdf-toolbar {
      background: #2d2a28;
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
    }
  }
  .pdf-tool {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: #d9d6d3;
    transition:
      background-color 200ms cubic-bezier(0.32, 0.72, 0, 1),
      color 200ms cubic-bezier(0.32, 0.72, 0, 1),
      transform 200ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .pdf-tool:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.09);
    color: #f2f1f0;
  }
  .pdf-tool:active:not(:disabled) {
    transform: scale(0.93);
  }
  .pdf-tool:disabled {
    color: #615d5a;
  }
  @media (prefers-reduced-motion: reduce) {
    .pdf-tool {
      transition: none;
    }
  }
</style>
