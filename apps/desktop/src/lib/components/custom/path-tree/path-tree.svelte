<script lang="ts">
  // Svelte wrapper over the vanilla @pierre/trees FileTree. Paths are tree paths:
  // directories end with "/". It knows nothing about where the paths come from.
  import { untrack } from "svelte";
  import { FileTree } from "@pierre/trees";
  import { trackPointerDrag } from "$lib/file-drag/pointer-drag.js";
  import { pathsToDrag, rowPathFromEvent, sameSelection, type PathTreeDragHandlers } from "./path-tree.js";

  type Props = {
    paths: string[];
    selected?: string[];
    /** Right-aligned row text, such as a size. */
    decorate?: (path: string) => string | null;
    search?: boolean;
    initialExpansion?: "open" | "closed" | number;
    onActivate?: (path: string) => void;
    dragHandlers?: PathTreeDragHandlers;
  };

  let {
    paths,
    selected = $bindable([]),
    decorate,
    search = false,
    initialExpansion = "closed",
    onActivate,
    dragHandlers,
  }: Props = $props();

  let host = $state<HTMLDivElement | null>(null);
  let tree: FileTree | null = null;
  let mountedPaths: readonly string[] | null = null;
  let cancelDrag = () => {};

  $effect(() => {
    const element = host;
    if (!element) return;
    const instance = untrack(
      () =>
        new FileTree({
          paths,
          initialExpansion,
          search,
          initialSelectedPaths: selected,
          flattenEmptyDirectories: true,
          dragAndDrop: false,
          renaming: false,
          itemHeight: 28,
          renderRowDecoration: ({ item }) => {
            const text = decorate?.(item.path);
            return text ? { text } : null;
          },
          onSelectionChange: (next) => {
            if (!sameSelection(next, selected)) selected = [...next];
          },
        }),
    );
    mountedPaths = untrack(() => paths);
    instance.render({ containerWrapper: element });
    tree = instance;
    return () => {
      cancelDrag();
      instance.cleanUp();
      tree = null;
      mountedPaths = null;
    };
  });

  $effect(() => {
    const next = paths;
    untrack(() => {
      if (!tree || next === mountedPaths) return;
      mountedPaths = next;
      tree.resetPaths(next);
    });
  });

  // Keeps the tree in step when the parent changes the selection, for example Clear.
  $effect(() => {
    const wanted = selected;
    untrack(() => {
      if (!tree || sameSelection(tree.getSelectedPaths(), wanted)) return;
      const keep = new Set(wanted);
      for (const path of tree.getSelectedPaths()) if (!keep.has(path)) tree.getItem(path)?.deselect();
      for (const path of wanted) {
        const item = tree.getItem(path);
        if (item && !item.isSelected()) item.select();
      }
    });
  });

  export function openSearch() {
    tree?.openSearch();
  }

  function handlePointerDown(event: PointerEvent) {
    const handlers = dragHandlers;
    if (!handlers) return;
    const pressed = rowPathFromEvent(event.composedPath());
    if (!pressed) return;
    let dragged: string[] = [];
    cancelDrag = trackPointerDrag(event, {
      onStart: (move) => {
        dragged = pathsToDrag(pressed, selected);
        handlers.start(dragged, move);
      },
      onMove: (move) => handlers.move(move),
      onDrop: (up) => handlers.drop(dragged, up),
      onCancel: () => handlers.cancel(),
    });
  }

  function handleDoubleClick(event: MouseEvent) {
    const path = rowPathFromEvent(event.composedPath());
    if (path) onActivate?.(path);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={host} class="path-tree" onpointerdowncapture={handlePointerDown} ondblclick={handleDoubleClick}></div>

<style>
  .path-tree {
    height: 100%;
    min-height: 0;
    --trees-bg-override: #1f1d1b;
    --trees-bg-muted-override: #2d2a28;
    --trees-fg-override: #e8e5e2;
    --trees-fg-muted-override: #9c9895;
    --trees-border-color-override: #3a3734;
    --trees-selected-bg-override: #3b3836;
    --trees-accent-override: #60a5fa;
    --trees-border-radius-override: 6px;
  }
</style>
