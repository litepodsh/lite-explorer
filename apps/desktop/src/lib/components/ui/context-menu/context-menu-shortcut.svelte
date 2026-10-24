<script lang="ts">
  import { cn, type WithElementRef } from "#lib/utils.js";
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
    "text-[var(--app-fg-faint)] group-focus/context-menu-item:text-[var(--app-accent-fg)] ml-auto text-[11px] tracking-normal",
    className,
  )}
  {...restProps}>
  {@render children?.()}
</span>
