<script lang="ts">
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FileIcon from "@lucide/svelte/icons/file";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import FolderInputIcon from "@lucide/svelte/icons/folder-input";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import StarIcon from "@lucide/svelte/icons/star";
  import { drag } from "$lib/file-drag/drag.svelte.js";

  const actions = {
    favorite: { label: "Favorites", icon: StarIcon },
    copy: { label: "Copy", icon: CopyIcon },
    move: { label: "Move", icon: FolderInputIcon },
  };
  const icons = { folder: FolderIcon, file: FileIcon, drive: HardDriveIcon };
</script>

{#if drag.ghost}
  {@const ghost = drag.ghost}
  {@const Icon = icons[ghost.icon]}
  <div class="drag-ghost" style:transform={`translate3d(${ghost.x + 14}px, ${ghost.y + 12}px, 0)`} aria-hidden="true">
    <div class="drag-ghost-card">
      <span class="drag-ghost-icon" data-icon={ghost.icon}><Icon /></span>
      <span class="drag-ghost-name">{ghost.name}</span>
      {#if ghost.action}
        {@const action = actions[ghost.action]}
        {#key ghost.action}
          <span class="drag-ghost-action" data-action={ghost.action}><action.icon />{action.label}</span>
        {/key}
      {/if}
    </div>
  </div>
{/if}
