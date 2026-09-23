<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
  import { cn } from "$lib/utils.js";
  import type { WithoutChildrenOrChild } from "$lib/utils.js";
  import ContextMenuPortal from "./context-menu-portal.svelte";
  import type { ComponentProps } from "svelte";

  let {
    ref = $bindable(null),
    portalProps,
    class: className,
    ...restProps
  }: ContextMenuPrimitive.SubContentProps & {
    portalProps?: WithoutChildrenOrChild<ComponentProps<typeof ContextMenuPortal>>;
  } = $props();
</script>

<!-- Portaled so the parent menu's backdrop-filter and overflow don't clip it. -->
<ContextMenuPortal {...portalProps}>
  <ContextMenuPrimitive.SubContent
    bind:ref
    data-slot="context-menu-sub-content"
    sideOffset={4}
    class={cn(
      "data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 data-[side=bottom]:slide-in-from-top-1 data-[side=top]:slide-in-from-bottom-1 data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1 text-[var(--app-fg)] min-w-48 rounded-[var(--app-radius)] p-1 shadow-[var(--app-shadow)] ring-1 ring-[var(--app-border)] z-50 max-h-(--bits-menu-content-available-height) origin-(--bits-menu-content-transform-origin) overflow-x-hidden overflow-y-auto animate-none! relative bg-[var(--app-surface)] font-[system-ui,-apple-system,sans-serif] **:data-[slot$=-item]:focus:bg-[var(--app-accent)] **:data-[slot$=-item]:data-highlighted:bg-[var(--app-accent)] **:data-[slot$=-item]:focus:text-[var(--app-accent-fg)] **:data-[slot$=-item]:data-highlighted:text-[var(--app-accent-fg)] **:data-[slot$=-separator]:bg-[var(--app-border)]",
      className,
    )}
    {...restProps} />
</ContextMenuPortal>
