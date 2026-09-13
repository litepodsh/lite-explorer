<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import { cn, type WithoutChild } from "$lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    inset,
    children,
    ...restProps
  }: WithoutChild<ContextMenuPrimitive.SubTriggerProps> & {
    inset?: boolean;
  } = $props();
</script>

<ContextMenuPrimitive.SubTrigger
  bind:ref
  data-slot="context-menu-sub-trigger"
  data-inset={inset}
  class={cn(
    "focus:bg-[#0a84ff] focus:text-white data-open:bg-[#0a84ff] data-open:text-white gap-2 min-h-[22px] rounded-md px-3 py-1 text-[13px] data-inset:pl-7 [&_svg:not([class*='size-'])]:size-4 flex cursor-default items-center outline-hidden select-none [&_svg]:pointer-events-none [&_svg]:shrink-0",
    className,
  )}
  {...restProps}>
  {@render children?.()}
  <ChevronRightIcon class="cn-rtl-flip ml-auto" />
</ContextMenuPrimitive.SubTrigger>
