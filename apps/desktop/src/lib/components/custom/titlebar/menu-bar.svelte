<script lang="ts">
  import * as Menubar from "$lib/components/ui/menubar/index.js";
  import MenuNodes from "./menu-nodes.svelte";
  import { appMenu } from "$lib/menu/app-menu.svelte.js";
  import type { MenuNode } from "$lib/menu/menu.js";
  import type { Platform } from "$lib/menu/accelerator.js";

  let { nodes, platform }: { nodes: MenuNode[]; platform: Platform } = $props();
  let root = $state<HTMLElement | null>(null);

  /** Focuses the first menu, like pressing Alt or F10 in a native Windows app. */
  export function focusFirst() {
    appMenu.rememberFocus();
    root?.querySelector<HTMLElement>("[data-slot='menubar-trigger']")?.focus();
  }
</script>

<Menubar.Root
  bind:ref={root}
  class="title-bar-menus h-full gap-0.5 rounded-none border-0 bg-transparent p-0"
  onpointerdowncapture={() => appMenu.rememberFocus()}>
  {#each nodes as node, index (index)}
    {#if node.type === "submenu"}
      <Menubar.Menu>
        <Menubar.Trigger class="title-bar-menu-trigger">{node.label}</Menubar.Trigger>
        <Menubar.Content class="title-bar-menu-content" sideOffset={2} alignOffset={0}>
          <MenuNodes nodes={node.children} {platform} />
        </Menubar.Content>
      </Menubar.Menu>
    {/if}
  {/each}
</Menubar.Root>
