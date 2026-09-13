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
      "data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 data-[side=bottom]:slide-in-from-top-1 data-[side=top]:slide-in-from-bottom-1 data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1 text-[#e8e5e2] min-w-48 rounded-lg p-1 shadow-[0_10px_40px_rgba(0,0,0,0.45)] ring-1 ring-white/10 z-50 max-h-(--bits-menu-content-available-height) origin-(--bits-menu-content-transform-origin) overflow-x-hidden overflow-y-auto animate-none! relative bg-[#2b2b2f]/80 backdrop-blur-2xl backdrop-saturate-125 font-[system-ui,-apple-system,sans-serif] **:data-[slot$=-item]:focus:bg-[#0a84ff] **:data-[slot$=-item]:data-highlighted:bg-[#0a84ff] **:data-[slot$=-item]:focus:text-white **:data-[slot$=-item]:data-highlighted:text-white **:data-[slot$=-separator]:bg-white/10",
      className,
    )}
    {...restProps} />
</ContextMenuPortal>
