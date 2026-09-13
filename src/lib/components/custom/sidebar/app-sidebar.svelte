<script lang="ts" module>
  import AppWindowIcon from "@lucide/svelte/icons/app-window";
  import Clock3Icon from "@lucide/svelte/icons/clock-3";
  import CloudIcon from "@lucide/svelte/icons/cloud";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import HouseIcon from "@lucide/svelte/icons/house";
  import ImageIcon from "@lucide/svelte/icons/image";
  import MonitorIcon from "@lucide/svelte/icons/monitor";
  import UsersIcon from "@lucide/svelte/icons/users";
  import type { Component } from "svelte";

  type Source = { label: string; icon: Component };
  const favorites: Source[] = [
    { label: "Applications", icon: AppWindowIcon },
    { label: "Desktop", icon: MonitorIcon },
    { label: "Documents", icon: FileTextIcon },
    { label: "Downloads", icon: DownloadIcon },
    { label: "Backup", icon: FolderIcon },
    { label: "Work", icon: FolderIcon },
    { label: "Movies", icon: ImageIcon },
  ];
  const locations: Source[] = [
    // { label: "iCloud Drive", icon: CloudIcon },
    // { label: "Proton Drive", icon: FolderIcon },
    { label: "sebastiangarzon", icon: HouseIcon },
    { label: "Macintosh HD", icon: HardDriveIcon },
  ];
  const tags = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"];
</script>

<script lang="ts">
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import type { ComponentProps } from "svelte";

  let {
    ref = $bindable(null),
    selected = $bindable("Recents"),
    collapsible = "offcanvas",
    open = true,
    width = 256,
    onResize,
    onResizeStart,
    onResizeEnd,
    onToggle,
    ...restProps
  }: ComponentProps<typeof Sidebar.Root> & {
    selected?: string;
    open?: boolean;
    width?: number;
    onResize?: (width: number) => void;
    onResizeStart?: () => void;
    onResizeEnd?: () => void;
    onToggle?: () => void;
  } = $props();

  function resizeSidebar(event: PointerEvent) {
    if (!onResize) return;
    onResizeStart?.();
    const startX = event.clientX;
    const startWidth = open ? width : 48;
    const move = (moveEvent: PointerEvent) => onResize(startWidth + moveEvent.clientX - startX);
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      onResizeEnd?.();
    };

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
    window.addEventListener("pointercancel", stop, { once: true });
  }
</script>

<Sidebar.Root bind:ref {collapsible} {...restProps}>
  <Sidebar.Header class="titlebar-spacer p-0" data-tauri-drag-region ondblclick={onToggle} />
  <Sidebar.Content class="finder-sidebar p-0">
    <nav class="finder-sources" aria-label="Finder sidebar">
      <button
        aria-label="Recents"
        title="Recents"
        class:active={selected === "Recents"}
        onclick={() => (selected = "Recents")}><Clock3Icon /> <span>Recents</span></button>
      <button
        aria-label="Shared"
        title="Shared"
        class:active={selected === "Shared"}
        onclick={() => (selected = "Shared")}><UsersIcon /> <span>Shared</span></button>
      <p>Favorites</p>
      {#each favorites as source (source.label)}
        <button
          aria-label={source.label}
          title={source.label}
          class:active={selected === source.label}
          onclick={() => (selected = source.label)}
          ><source.icon /> <span>{source.label}</span></button>
      {/each}
      <p>Locations</p>
      {#each locations as source (source.label)}
        <button
          aria-label={source.label}
          title={source.label}
          class:active={selected === source.label}
          onclick={() => (selected = source.label)}
          ><source.icon /> <span>{source.label}</span></button>
      {/each}
      <p>Tags</p>
      {#each tags as tag (tag)}
        <button
          aria-label={tag}
          title={tag}
          class:active={selected === tag}
          onclick={() => (selected = tag)}
          ><i
            class:gray={tag === "Gray"}
            style:background-color={tag === "Gray" ? undefined : tag.toLowerCase()}></i
          ><span>{tag}</span></button>
      {/each}
    </nav>
  </Sidebar.Content>
  <Sidebar.Footer class="hidden" />
  <Sidebar.Rail onResizeStart={resizeSidebar} {onToggle} />
</Sidebar.Root>
