<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
  import { cn } from "$lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    inset,
    variant = "default",
    ...restProps
  }: ContextMenuPrimitive.ItemProps & {
    inset?: boolean;
    variant?: "default" | "destructive";
  } = $props();
</script>

<ContextMenuPrimitive.Item
  bind:ref
  data-slot="context-menu-item"
  data-inset={inset}
  data-variant={variant}
  class={cn(
    "focus:bg-[#0a84ff] focus:text-white data-[variant=destructive]:text-[#ff453a] data-[variant=destructive]:focus:bg-[#ff453a] data-[variant=destructive]:focus:text-white! data-[variant=destructive]:data-highlighted:text-white! data-[variant=destructive]:*:[svg]:text-[#ff453a] focus:*:[svg]:text-white data-[variant=destructive]:focus:*:[svg]:text-white! data-[variant=destructive]:data-highlighted:*:[svg]:text-white! gap-2 min-h-[22px] rounded-md px-3 py-1 text-[13px] data-inset:pl-7 [&_svg:not([class*='size-'])]:size-4 group/context-menu-item relative flex cursor-default items-center outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0",
    className,
  )}
  {...restProps} />
