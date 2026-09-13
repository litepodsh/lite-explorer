<script lang="ts">
  import { cn, type WithElementRef } from "$lib/utils.js";
  import type { HTMLAttributes } from "svelte/elements";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLSpanElement>> = $props();
</script>

<span
  bind:this={ref}
  data-slot="context-menu-shortcut"
  class={cn(
    "text-white/40 group-focus/context-menu-item:text-white ml-auto text-[11px] tracking-normal",
    className,
  )}
  {...restProps}>
  {@render children?.()}
</span>
