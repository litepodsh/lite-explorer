<script lang="ts">
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FileIcon from "@lucide/svelte/icons/file";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import FolderInputIcon from "@lucide/svelte/icons/folder-input";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import PackageOpenIcon from "@lucide/svelte/icons/package-open";
  import StarIcon from "@lucide/svelte/icons/star";
  import { drag } from "$lib/file-drag/drag.svelte.js";

  const actions = {
    favorite: { label: "Favorites", icon: StarIcon },
    copy: { label: "Copy", icon: CopyIcon },
    move: { label: "Move", icon: FolderInputIcon },
    extract: { label: "Extract", icon: PackageOpenIcon },
  };
  const icons = { folder: FolderIcon, file: FileIcon, drive: HardDriveIcon };
</script>

{#if drag.ghost}
  {@const ghost = drag.ghost}
  {@const Icon = icons[ghost.icon]}
  <div class="drag-ghost" style:transform={`translate3d(${ghost.x + 14}px, ${ghost.y + 12}px, 0)`} aria-hidden="true">
    <div class="drag-ghost-stack">
      {#if (ghost.count ?? 1) > 1}
        <span class="drag-ghost-layer" data-depth="2"></span>
        <span class="drag-ghost-layer" data-depth="1"></span>
      {/if}
      <div class="drag-ghost-card">
        <span class="drag-ghost-icon" data-icon={ghost.icon}><Icon /></span>
        <span class="drag-ghost-name">{ghost.name}</span>
        {#if (ghost.count ?? 1) > 1}<span class="drag-ghost-count">{ghost.count}</span>{/if}
        {#if ghost.action}
          {@const action = actions[ghost.action]}
          {#key ghost.action}
            <span class="drag-ghost-action" data-action={ghost.action}><action.icon />{action.label}</span>
          {/key}
        {/if}
      </div>
    </div>
  </div>
{/if}
