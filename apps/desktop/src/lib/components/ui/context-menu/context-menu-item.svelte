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
    "focus:bg-[var(--app-accent)] focus:text-[var(--app-accent-fg)] data-[variant=destructive]:text-[var(--app-danger)] data-[variant=destructive]:focus:bg-[var(--app-danger)] data-[variant=destructive]:focus:text-[var(--app-accent-fg)]! data-[variant=destructive]:data-highlighted:text-[var(--app-accent-fg)]! data-[variant=destructive]:*:[svg]:text-[var(--app-danger)] focus:*:[svg]:text-[var(--app-accent-fg)] data-[variant=destructive]:focus:*:[svg]:text-[var(--app-accent-fg)]! data-[variant=destructive]:data-highlighted:*:[svg]:text-[var(--app-accent-fg)]! gap-2 min-h-[22px] rounded-md px-3 py-1 text-[13px] data-inset:pl-7 [&_svg:not([class*='size-'])]:size-4 group/context-menu-item relative flex cursor-default items-center outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0",
    className,
  )}
  {...restProps} />
