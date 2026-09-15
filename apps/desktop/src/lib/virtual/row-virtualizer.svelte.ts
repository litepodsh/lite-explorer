import { createVirtualizer, elementScroll, observeElementOffset, observeElementRect, type Virtualizer } from "@tanstack/svelte-virtual";

type RowVirtualizerOptions = {
  count: () => number;
  estimateSize: () => number;
  overscan?: number;
  /** Distance (px) from the top of the scroll content to the first row, e.g. a sticky header. */
  scrollMargin?: () => number;
  /** Distance (px) at the top of the viewport covered by overlays, kept clear when scrolling to a row. */
  scrollPaddingStart?: () => number;
  getScrollElement: () => HTMLElement | null;
};

export type RowVirtualizer = Virtualizer<HTMLElement, HTMLElement>;

/**
 * Reusable Svelte 5 wrapper around TanStack Virtual's `createVirtualizer`.
 *
 * TanStack caches the scroll element on first read, so if the element is still
 * `null` when the virtualizer mounts it never recovers. We therefore create the
 * virtualizer lazily via a `$derived` that depends on the scroll element, and
 * push `count`/`estimateSize`/`overscan` changes through `setOptions` (the
 * adapter does not reactively track getter options).
 *
 * The returned object exposes state through getters: returning the `$state`
 * values directly would snapshot them (`[]`/`0`) and never update. Don't
 * destructure it.
 *
 * Rows are positioned at `start - scrollMargin` relative to the rows container.
 */
export function createRowVirtualizer(opts: RowVirtualizerOptions) {
  const getOverscan = () => opts.overscan ?? 10;
  const getScrollMargin = () => opts.scrollMargin?.() ?? 0;
  const scrollElement = $derived(opts.getScrollElement());

  const virtualizer = $derived.by(() => {
    const el = scrollElement;
    if (!el) return undefined;
    return createVirtualizer<HTMLElement, HTMLElement>({
      count: 0,
      getScrollElement: () => el,
      estimateSize: opts.estimateSize,
      overscan: getOverscan(),
    });
  });

  // `$state.raw`: these are replaced wholesale on every scroll notification, so
  // deep-proxying each virtual item would only add per-frame overhead.
  let instance = $state.raw<RowVirtualizer | undefined>(undefined);
  let virtualItems = $state.raw<ReturnType<RowVirtualizer["getVirtualItems"]>>([]);
  let totalSize = $state(0);

  $effect(() => {
    const v = virtualizer;
    if (!v) {
      instance = undefined;
      return;
    }
    const unsub = v.subscribe((inst) => {
      instance = inst as RowVirtualizer;
      virtualItems = inst.getVirtualItems();
      totalSize = inst.getTotalSize();
    });
    return unsub;
  });

  // TanStack does not invalidate cached measurements when `estimateSize`
  // changes (e.g. list -> grid), so force a re-measure in that case.
  let lastEstimate: number | undefined;

  $effect(() => {
    const inst = instance;
    if (!inst) return;
    const estimate = opts.estimateSize();
    inst.setOptions({
      count: opts.count(),
      estimateSize: opts.estimateSize,
      overscan: getOverscan(),
      scrollMargin: getScrollMargin(),
      scrollPaddingStart: opts.scrollPaddingStart?.() ?? 0,
      getScrollElement: () => scrollElement,
      scrollToFn: elementScroll,
      observeElementRect,
      observeElementOffset,
    });
    if (lastEstimate !== undefined && lastEstimate !== estimate) inst.measure();
    lastEstimate = estimate;
  });

  return {
    get virtualizer() {
      return virtualizer;
    },
    get virtualItems() {
      return virtualItems;
    },
    get totalSize() {
      return totalSize;
    },
  };
}
