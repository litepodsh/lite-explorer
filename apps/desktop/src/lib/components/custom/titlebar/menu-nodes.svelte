<script lang="ts">
  import * as Menubar from "$lib/components/ui/menubar/index.js";
  import MenuNodes from "./menu-nodes.svelte";
  import { appMenu } from "$lib/menu/app-menu.svelte.js";
  import { shortcutLabel, type MenuNode } from "$lib/menu/menu.js";
  import type { Platform } from "$lib/menu/accelerator.js";

  let { nodes, platform }: { nodes: MenuNode[]; platform: Platform } = $props();
</script>

{#each nodes as node, index (index)}
  {#if node.type === "separator"}
    <Menubar.Separator />
  {:else if node.type === "submenu"}
    <Menubar.Sub>
      <Menubar.SubTrigger>{node.label}</Menubar.SubTrigger>
      <Menubar.SubContent class="title-bar-menu-content">
        <MenuNodes nodes={node.children} {platform} />
      </Menubar.SubContent>
    </Menubar.Sub>
  {:else}
    {@const shortcut = shortcutLabel(node, platform)}
    {#if node.type === "check"}
      <Menubar.CheckboxItem
        checked={node.checked}
        disabled={!node.enabled}
        onSelect={() => void appMenu.run(node)}>
        {node.label}
        {#if shortcut}<Menubar.Shortcut>{shortcut}</Menubar.Shortcut>{/if}
      </Menubar.CheckboxItem>
    {:else}
      <Menubar.Item
        disabled={node.type === "item" && !node.enabled}
        onSelect={() => void appMenu.run(node)}>
        {node.label}
        {#if shortcut}<Menubar.Shortcut>{shortcut}</Menubar.Shortcut>{/if}
      </Menubar.Item>
    {/if}
  {/if}
{/each}
