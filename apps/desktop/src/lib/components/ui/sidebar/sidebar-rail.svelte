<script lang="ts">
  import { cn, type WithElementRef } from "$lib/utils.js";
  import { useSidebar } from "./context.svelte.js";
  import type { HTMLAttributes } from "svelte/elements";

  let {
    ref = $bindable(null),
    class: className,
    children,
    onResizeStart,
    onToggle,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLButtonElement>, HTMLButtonElement> & {
    onResizeStart?: (event: PointerEvent) => void;
    onToggle?: () => void;
  } = $props();

  const sidebar = useSidebar();
  let resizing = false;
  let pointerStart: { x: number; y: number } | undefined;

  function handlePointerdown(event: PointerEvent) {
    if (!onResizeStart || event.button !== 0) return;
    pointerStart = { x: event.clientX, y: event.clientY };
    (event.currentTarget as HTMLButtonElement).setPointerCapture(event.pointerId);
    onResizeStart(event);
  }

  function handlePointermove(event: PointerEvent) {
    if (!pointerStart) return;
    resizing ||= Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 3;
  }

  function handlePointerup(event: PointerEvent) {
    pointerStart = undefined;
    (event.currentTarget as HTMLButtonElement).releasePointerCapture(event.pointerId);
  }

  function handleClick(event: MouseEvent) {
    if (resizing) {
      resizing = false;
      event.preventDefault();
      return;
    }
    if (!onResizeStart) sidebar.toggle();
  }

  function handleDoubleClick() {
    (onToggle ?? sidebar.toggle)();
  }
</script>

<button
  bind:this={ref}
  type="button"
  data-sidebar="rail"
  data-slot="sidebar-rail"
  aria-label="Toggle Sidebar"
  tabindex={-1}
  onpointerdown={handlePointerdown}
  onpointermove={handlePointermove}
  onpointerup={handlePointerup}
  onclick={handleClick}
  ondblclick={handleDoubleClick}
  title="Toggle Sidebar"
  class={cn(
    "hover:after:bg-sidebar-border absolute inset-y-0 z-20 hidden w-5 touch-none select-none transition-all ease-linear group-data-[side=left]:-right-5 group-data-[side=right]:left-0 after:absolute after:inset-y-0 after:start-1/2 after:w-[2px] sm:flex ltr:-translate-x-1/2 rtl:-translate-x-1/2",
    "in-data-[side=left]:cursor-w-resize in-data-[side=right]:cursor-e-resize",
    "[[data-side=left][data-state=collapsed]_&]:cursor-e-resize [[data-side=right][data-state=collapsed]_&]:cursor-w-resize",
    "group-data-[collapsible=offcanvas]:translate-x-0 group-data-[collapsible=offcanvas]:after:left-full hover:group-data-[collapsible=offcanvas]:bg-sidebar",
    "[[data-side=left][data-collapsible=offcanvas]_&]:-right-2",
    "[[data-side=right][data-collapsible=offcanvas]_&]:-left-2",
    className,
  )}
  {...restProps}>
  {@render children?.()}
</button>
