<script lang="ts">
  import PlusIcon from "@lucide/svelte/icons/plus";
  import XIcon from "@lucide/svelte/icons/x";
  import { flip } from "svelte/animate";
  import { quintOut } from "svelte/easing";
  import { prefersReducedMotion } from "svelte/motion";
  import type { Tab } from "$lib/tabs/tabs.js";
  import { drag as dragState } from "$lib/file-drag/drag.svelte.js";

  let {
    tabs,
    activeId,
    paneId = "",
    onSelect,
    onClose,
    onMove,
    onCrossPaneDrop,
    onNew,
  }: {
    tabs: Tab[];
    activeId: string;
    paneId?: string;
    onSelect: (id: string) => void;
    onClose: (id: string) => void;
    onMove: (from: number, to: number) => void;
    onCrossPaneDrop?: (fromPaneId: string, toPaneId: string, fromIndex: number, toIndex: number) => void;
    onNew: () => void;
  } = $props();

  const DRAG_THRESHOLD = 4;

  let open = $derived(tabs.length > 1);
  let activeIndex = $derived(Math.max(0, tabs.findIndex((tab) => tab.id === activeId)));
  let flipDuration = $derived(prefersReducedMotion.current ? 0 : 200);

  const tabElements: Record<string, HTMLElement> = {};
  let drag = $state<{ id: string; originX: number; moved: boolean } | null>(null);
  let dragX = $state(0);
  let swallowClick = false;

  function trackTab(id: string) {
    return (node: HTMLElement) => {
      tabElements[id] = node;
      return () => {
        if (tabElements[id] === node) delete tabElements[id];
      };
    };
  }

  $effect(() => {
    if (!open) return;
    tabElements[activeId]?.scrollIntoView({
      inline: "nearest",
      block: "nearest",
      behavior: prefersReducedMotion.current ? "auto" : "smooth",
    });
  });

  // Window listeners instead of pointer capture: reordering the keyed each moves the
  // dragged DOM node, which would drop capture mid-drag.
  function startDrag(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    onSelect(id);
    drag = { id, originX: event.clientX, moved: false };
    dragState.tab = { paneId, id };
    window.addEventListener("pointermove", moveDrag);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
  }

  function moveDrag(event: PointerEvent) {
    if (!drag) return;
    const offset = event.clientX - drag.originX;
    if (!drag.moved && Math.abs(offset) < DRAG_THRESHOLD) return;
    drag.moved = true;
    const id = drag.id;
    const index = tabs.findIndex((tab) => tab.id === id);
    const width = tabElements[id]?.offsetWidth ?? 0;
    if (index < 0 || width === 0) return;
    dragX = Math.min((tabs.length - 1 - index) * width, Math.max(-index * width, offset));
    const shift = Math.round(dragX / width);
    if (shift === 0) return;
    onMove(index, index + shift);
    // The tab now sits in a new slot; rebase so it stays under the cursor.
    drag.originX += shift * width;
    dragX -= shift * width;
  }

  function removeDragListeners() {
    window.removeEventListener("pointermove", moveDrag);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", endDrag);
  }

  function endDrag(event?: PointerEvent) {
    removeDragListeners();
    const sourceId = drag?.id;
    const moved = drag?.moved ?? false;
    dragState.tab = null;
    if (moved && event && sourceId) {
      const element = document.elementFromPoint(event.clientX, event.clientY);
      const bar = element?.closest("[data-pane-tab-bar]") as HTMLElement | null;
      const targetPane = bar?.dataset.paneTabBar;
      if (targetPane && targetPane !== paneId) {
        const fromIndex = tabs.findIndex((tab) => tab.id === sourceId);
        const tabElements = [...bar!.querySelectorAll<HTMLElement>("[role='tab']")];
        let toIndex = tabElements.findIndex(
          (tab) => event.clientX < tab.getBoundingClientRect().left + tab.getBoundingClientRect().width / 2,
        );
        if (toIndex < 0) toIndex = tabElements.length;
        onCrossPaneDrop?.(paneId, targetPane, fromIndex, toIndex);
        swallowClick = true;
        drag = null;
        dragX = 0;
        return;
      }
    }
    swallowClick = moved;
    drag = null;
    dragX = 0;
  }

  $effect(() => removeDragListeners);

  function onStripClickCapture(event: MouseEvent) {
    if (!swallowClick) return;
    swallowClick = false;
    event.preventDefault();
    event.stopPropagation();
  }

  function tabTransform(id: string): string | undefined {
    if (drag?.id !== id || !drag.moved) return undefined;
    return `translateX(${dragX}px) scale(1.02)`;
  }

  function indicatorTransform(): string {
    const offset = drag?.id === activeId && drag.moved ? dragX : 0;
    return `translateX(calc(${activeIndex * 100}% + ${offset}px))`;
  }
</script>

<div class="tab-bar" class:open inert={!open} data-pane-tab-bar={paneId}>
  <div class="tab-bar-inner">
    <div class="tab-bar-row">
      <div class="tab-track">
        <div
          class="tab-strip"
          role="tablist"
          aria-label="Tabs"
          tabindex="-1"
          style:--tab-count={tabs.length}
          onclickcapture={onStripClickCapture}>
          <span
            class="tab-indicator"
            class:dragging={drag?.id === activeId && drag.moved}
            style:transform={indicatorTransform()}
            aria-hidden="true"></span>
          {#each tabs as tab (tab.id)}
            <div
              {@attach trackTab(tab.id)}
              animate:flip={{ duration: drag?.id === tab.id ? 0 : flipDuration, easing: quintOut }}
              class="tab"
              class:active={tab.id === activeId}
              class:dragging={drag?.id === tab.id && drag.moved}
              style:transform={tabTransform(tab.id)}
              role="tab"
              tabindex={tab.id === activeId ? 0 : -1}
              aria-selected={tab.id === activeId}
              title={tab.location.path || tab.location.name}
              onpointerdown={(event) => startDrag(event, tab.id)}
              onkeydown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onSelect(tab.id);
                }
              }}>
              <button
                type="button"
                class="tab-close"
                aria-label={`Close ${tab.location.name}`}
                onpointerdown={(event) => event.stopPropagation()}
                onclick={(event) => {
                  event.stopPropagation();
                  onClose(tab.id);
                }}><XIcon /></button>
              <span class="tab-label">{tab.location.name}</span>
            </div>
          {/each}
        </div>
      </div>
      <button type="button" class="tab-new" aria-label="New Tab" title="New Tab" onclick={onNew}
        ><PlusIcon /></button>
    </div>
  </div>
</div>

<style>
  .tab-bar {
    display: grid;
    grid-template-rows: 0fr;
    opacity: 0;
    padding-inline: 16px;
    transition:
      grid-template-rows 200ms cubic-bezier(0.2, 0, 0, 1),
      opacity 200ms cubic-bezier(0.2, 0, 0, 1);
  }
  .tab-bar.open {
    grid-template-rows: 1fr;
    opacity: 1;
  }
  .tab-bar-inner {
    min-height: 0;
    overflow: hidden;
  }
  .tab-bar-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: 8px;
  }
  .tab-track {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 2px;
    border-radius: 999px;
    background: #1d1b1a;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.04);
  }
  .tab-track::-webkit-scrollbar {
    display: none;
  }
  .tab-strip {
    position: relative;
    display: grid;
    grid-template-columns: repeat(var(--tab-count), minmax(100px, 1fr));
    width: max(100%, calc(var(--tab-count) * 100px));
    height: 24px;
    outline: none;
  }
  .tab-indicator {
    position: absolute;
    inset-block: 0;
    left: 0;
    width: calc(100% / var(--tab-count));
    border-radius: 999px;
    background: #3a3735;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.08),
      inset 0 1px 0 rgb(255 255 255 / 0.06),
      0 1px 2px rgb(20 18 16 / 0.4);
    pointer-events: none;
    transition:
      transform 240ms cubic-bezier(0.2, 0, 0, 1),
      width 240ms cubic-bezier(0.2, 0, 0, 1);
  }
  .tab {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    padding-inline: 24px;
    border-radius: 999px;
    color: #a5a09d;
    font-size: 12px;
    cursor: default;
    user-select: none;
    outline: none;
    transition:
      color 150ms cubic-bezier(0.2, 0, 0, 1),
      transform 200ms cubic-bezier(0.2, 0, 0, 1);
  }
  .tab:hover {
    color: #d9d6d3;
  }
  .tab.active {
    color: #f2f1f0;
    font-weight: 600;
  }
  .tab:focus-visible {
    box-shadow: 0 0 0 2px rgb(10 132 255 / 0.6);
  }
  .tab.dragging {
    z-index: 2;
    background: #2e2b29;
    box-shadow: 0 2px 6px rgb(20 18 16 / 0.45);
  }
  .tab.active.dragging {
    background: transparent;
    box-shadow: none;
  }
  .tab.dragging,
  .tab-indicator.dragging {
    transition: none;
  }
  .tab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    animation: tab-enter 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  @keyframes tab-enter {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }
  .tab-close {
    position: absolute;
    top: 50%;
    left: 4px;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: #d9d6d3;
    opacity: 0;
    translate: 0 -50%;
    transition:
      opacity 120ms cubic-bezier(0.2, 0, 0, 1),
      background-color 120ms cubic-bezier(0.2, 0, 0, 1);
  }
  .tab:hover .tab-close,
  .tab-close:focus-visible {
    opacity: 1;
  }
  .tab-close:hover {
    background: #4a4644;
  }
  .tab-close :global(svg) {
    width: 10px;
    height: 10px;
    stroke-width: 1.8;
  }
  .tab-new {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: #1d1b1a;
    color: #d9d6d3;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.04);
    transition:
      background-color 150ms cubic-bezier(0.2, 0, 0, 1),
      transform 150ms cubic-bezier(0.2, 0, 0, 1);
  }
  .tab-new:hover {
    background: #373432;
  }
  .tab-new:active {
    transform: scale(0.96);
  }
  .tab-new :global(svg) {
    width: 14px;
    height: 14px;
    stroke-width: 1.8;
  }
  @media (prefers-reduced-motion: reduce) {
    .tab-bar,
    .tab-indicator,
    .tab,
    .tab-close,
    .tab-new {
      transition: none;
    }
    .tab-label {
      animation: none;
    }
  }
</style>
