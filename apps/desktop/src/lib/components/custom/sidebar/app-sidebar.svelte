<script lang="ts" module>
  import Clock3Icon from "@lucide/svelte/icons/clock-3";
  import CloudIcon from "@lucide/svelte/icons/cloud";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import NetworkIcon from "@lucide/svelte/icons/network";
  import ServerIcon from "@lucide/svelte/icons/server";
  import HouseIcon from "@lucide/svelte/icons/house";
  import UsersIcon from "@lucide/svelte/icons/users";
  import SearchIcon from "@lucide/svelte/icons/search";

  import type { Location } from "$lib/tabs/tabs.js";
  export type { Location };
  const tags = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"];
</script>

<script lang="ts">
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import ClipboardCopyIcon from "@lucide/svelte/icons/clipboard-copy";
  import EjectIcon from "@lucide/svelte/icons/eject";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import LockIcon from "@lucide/svelte/icons/lock";
  import PanelLeftIcon from "@lucide/svelte/icons/panel-left";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import StarIcon from "@lucide/svelte/icons/star";
  import StarOffIcon from "@lucide/svelte/icons/star-off";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import { drag } from "$lib/file-drag/drag.svelte.js";
  import { canFavorite, favoriteDropIndexAt, moveTo } from "$lib/favorites/favorites.js";
  import { trackPointerDrag } from "$lib/file-drag/pointer-drag.js";
  import { atWindowEdge, startNativeDrag } from "$lib/file-drag/native-drag.js";
  import { isNetworkProtocol } from "$lib/remote/network-locations.js";
  import type { LocationStatus } from "$lib/remote/network-status.svelte.js";
  import type { ComponentProps } from "svelte";

  let {
    ref = $bindable(null),
    selected = "Overview",
    favorites = [],
    locations = [],
    collapsible = "offcanvas",
    open = true,
    width = 256,
    onResize,
    onResizeStart,
    onResizeEnd,
    onToggle,
    onOpen,
    onAddLocation,
    onRemoveLocation,
    onEditLocation,
    onCopyLocationAddress,
    onDisconnectLocation,
    onAddFavorite,
    onRemoveFavorite,
    onReorderFavorites,
    statuses = {},
    onOpenPalette,
    brand = false,
    ...restProps
  }: ComponentProps<typeof Sidebar.Root> & {
    selected?: string;
    favorites?: Location[];
    locations?: Location[];
    open?: boolean;
    width?: number;
    onResize?: (width: number) => void;
    onResizeStart?: () => void;
    onResizeEnd?: () => void;
    onToggle?: () => void;
    onOpen?: (location: Location) => void;
    onAddLocation?: () => void;
    onRemoveLocation?: (location: Location) => void;
    onEditLocation?: (location: Location) => void;
    onCopyLocationAddress?: (location: Location) => void;
    onDisconnectLocation?: (location: Location) => void;
    /** Adds a local folder to the favorites at `index`. */
    onAddFavorite?: (path: string, index: number) => void;
    onRemoveFavorite?: (location: Location) => void;
    onReorderFavorites?: (paths: string[]) => void;
    /** Connection state of network locations, by path. */
    statuses?: Record<string, LocationStatus>;
    onOpenPalette?: () => void;
    /** Show the logo, name and sidebar toggle in the header (Windows and Linux). */
    brand?: boolean;
  } = $props();

  let externalDroppable = $derived(
    drag.entry !== null &&
      canFavorite({ is_directory: drag.entry.isDirectory, path: drag.entry.path, kind: drag.entry.kind }),
  );
  let draggedFavoriteIndex = $derived(
    favorites.findIndex((favorite) => favorite.path === (drag.favorite ?? drag.entry?.path)),
  );
  // Dropping a favorite next to itself changes nothing, so no insertion line is shown there.
  let dropLineAt = $derived(
    drag.favoriteDropAt !== null &&
      draggedFavoriteIndex >= 0 &&
      (drag.favoriteDropAt === draggedFavoriteIndex || drag.favoriteDropAt === draggedFavoriteIndex + 1)
      ? null
      : drag.favoriteDropAt,
  );

  /** Inserts `path` at `index`, or moves it there when it is already a favorite. */
  function dropOnFavorites(path: string, index: number) {
    const paths = favorites.map((favorite) => favorite.path);
    const from = paths.indexOf(path);
    if (from < 0) return onAddFavorite?.(path, index);
    const reordered = moveTo(paths, from, index);
    if (reordered.some((favorite, position) => favorite !== paths[position])) onReorderFavorites?.(reordered);
  }

  // File lists drop folders on the favorites through the shared registry.
  $effect(() => {
    drag.favoritesDrop = dropOnFavorites;
    return () => {
      if (drag.favoritesDrop === dropOnFavorites) drag.favoritesDrop = null;
    };
  });

  let cancelReorder = () => {};
  $effect(() => () => cancelReorder());

  function startReorder(event: PointerEvent, favorite: Location) {
    cancelReorder = trackPointerDrag(event, {
      onStart: () => (drag.favorite = favorite.path),
      onMove: (move) => {
        // At the window edge the drag becomes a native one, so the folder can be dropped in other apps.
        if (atWindowEdge(move.clientX, move.clientY, window.innerWidth, window.innerHeight)) {
          cancelReorder();
          startNativeDrag(favorite.path, favorite.name, "folder");
          return;
        }
        drag.favoriteDropAt = favoriteDropIndexAt(move.clientX, move.clientY);
        drag.ghost = { x: move.clientX, y: move.clientY, name: favorite.name, icon: "folder", action: null };
      },
      onDrop: (up) => {
        const index = favoriteDropIndexAt(up.clientX, up.clientY);
        endReorder();
        if (index !== null) dropOnFavorites(favorite.path, index);
      },
      onCancel: endReorder,
    });
  }

  function endReorder() {
    drag.favorite = null;
    drag.favoriteDropAt = null;
    drag.ghost = null;
  }

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
  {#if brand}
    <Sidebar.Header class="titlebar-spacer sidebar-brand p-0" data-tauri-drag-region>
      <img src="/app-icon.png" alt="" width="18" height="18" />
      <span>Lite Explorer</span>
      <button aria-label="Toggle sidebar" title="Toggle sidebar" onclick={onToggle}><PanelLeftIcon /></button>
    </Sidebar.Header>
  {:else}
    <Sidebar.Header class="titlebar-spacer p-0" data-tauri-drag-region ondblclick={onToggle} />
  {/if}
  <Sidebar.Content class="finder-sidebar p-0">
    <nav class="finder-sources" aria-label="Finder sidebar">
      <button
        aria-label="Go to Folder"
        title="Go to Folder (⇧⌘P)"
        onclick={() => onOpenPalette?.()}
        ><SearchIcon /> <span>Go to Folder…</span></button>
      <button
        aria-label="Overview"
        title="Overview"
        class:active={selected === "Overview"}
        onclick={() => onOpen?.({ name: "Overview", path: "", kind: "overview" })}><GaugeIcon /> <span>Overview</span></button>
      <button
        aria-label="Recents"
        title="Recents"
        class:active={selected === "Recents"}
        onclick={() => onOpen?.({ name: "Recents", path: "", kind: "recents" })}><Clock3Icon /> <span>Recents</span></button>
      <button
        aria-label="Shared"
        title="Shared"
        class:active={selected === "Shared"}
        onclick={() => onOpen?.({ name: "Shared", path: "", kind: "shared" })}><UsersIcon /> <span>Shared</span></button>
      <p>Favorites</p>
      <div class="favorites-drop" role="list" aria-label="Favorites" data-favorites-drop={favorites.length}>
        {#each favorites as favorite, index (favorite.path)}
          {#if dropLineAt === index}<span class="favorite-drop-line" aria-hidden="true"></span>{/if}
          <ContextMenu.Root>
            <ContextMenu.Trigger>
              {#snippet child({ props })}
                <div
                  {...props}
                  role="listitem"
                  class="location-row favorite-row"
                  class:dragging={drag.favorite === favorite.path}
                  data-favorite-index={index}
                  data-favorite-path={favorite.path}>
                  <button
                    aria-label={favorite.name}
                    title={favorite.path}
                    class:active={selected === favorite.name}
                    onpointerdown={(event) => startReorder(event, favorite)}
                    onclick={() => onOpen?.(favorite)}><FolderIcon /> <span>{favorite.name}</span></button>
                  <button
                    class="location-trail favorite-star"
                    aria-label={`Remove ${favorite.name} from Favorites`}
                    title="Remove from Favorites"
                    onclick={() => onRemoveFavorite?.(favorite)}><StarIcon /></button>
                </div>
              {/snippet}
            </ContextMenu.Trigger>
            <ContextMenu.Content>
              <ContextMenu.Item onSelect={() => onRemoveFavorite?.(favorite)}>
                <StarOffIcon /> Remove from Favorites
              </ContextMenu.Item>
            </ContextMenu.Content>
          </ContextMenu.Root>
        {/each}
        {#if dropLineAt !== null && dropLineAt === favorites.length && favorites.length}
          <span class="favorite-drop-line" aria-hidden="true"></span>
        {/if}
        {#if externalDroppable && !favorites.length}
          <div class="favorite-drop-hint" class:over={drag.favoriteDropAt === 0}>Drop to add to Favorites</div>
        {/if}
      </div>
      <div class="finder-section">
        <p>Locations</p>
        {#if onAddLocation}
          <button
            class="section-add"
            aria-label="Add location"
            title="Add location"
            onclick={() => onAddLocation()}><PlusIcon /></button>
        {/if}
      </div>
      {#each locations as location (location.path)}
        {#if location.kind === "s3" || isNetworkProtocol(location.kind)}
          {@const status = statuses[location.path]}
          <ContextMenu.Root>
            <ContextMenu.Trigger>
              {#snippet child({ props })}
                <div {...props} class="location-row" data-state={status?.state ?? "idle"}>
                  <button
                    aria-label={location.name}
                    title={status?.state === "locked" ? `${location.name}: password needed` : location.name}
                    class:active={selected === location.name}
                    aria-busy={status?.state === "connecting"}
                    onclick={() => onOpen?.(location)}
                    >{#if status?.state === "connecting"}<LoaderCircleIcon class="location-spinner" />{:else if location.kind === "s3"}<CloudIcon />{:else if location.kind === "webdav"}<GlobeIcon />{:else if location.kind === "sftp" || location.kind === "ftp"}<ServerIcon />{:else}<NetworkIcon />{/if}<span
                      >{location.name}</span
                    ></button>
                  {#if status?.state === "connected"}
                    <button
                      class="location-trail"
                      aria-label={`Disconnect ${location.name}`}
                      title="Disconnect"
                      onclick={() => onDisconnectLocation?.(location)}><EjectIcon /></button>
                  {:else if status?.state === "error"}
                    <button
                      class="location-trail error"
                      aria-label={`Retry ${location.name}`}
                      title="Retry"
                      onclick={() => onOpen?.(location)}><RotateCwIcon /></button>
                  {:else if status?.state === "locked"}
                    <span class="location-trail muted" aria-hidden="true"><LockIcon /></span>
                  {/if}
                </div>
                {#if status?.state === "error"}
                  <div class="location-error" role="status">{status.message}</div>
                {/if}
              {/snippet}
            </ContextMenu.Trigger>
            <ContextMenu.Content>
              {#if isNetworkProtocol(location.kind)}
                {#if status?.state === "connected"}
                  <ContextMenu.Item onSelect={() => onDisconnectLocation?.(location)}><EjectIcon /> Disconnect</ContextMenu.Item>
                {/if}
                <ContextMenu.Item onSelect={() => onEditLocation?.(location)}><PencilIcon /> Edit…</ContextMenu.Item>
                <ContextMenu.Item onSelect={() => onCopyLocationAddress?.(location)}>
                  <ClipboardCopyIcon /> Copy Address
                </ContextMenu.Item>
                <ContextMenu.Separator />
              {/if}
              <ContextMenu.Item variant="destructive" onSelect={() => onRemoveLocation?.(location)}>
                <Trash2Icon /> Remove Location
              </ContextMenu.Item>
            </ContextMenu.Content>
          </ContextMenu.Root>
        {:else}
          <button
            aria-label={location.name}
            title={location.path}
            class:active={selected === location.name}
            onclick={() => onOpen?.(location)}
            >{#if location.kind === "home"}<HouseIcon />{:else}<HardDriveIcon />{/if}<span>{location.name}</span></button>
        {/if}
      {/each}
      <p>Tags</p>
      {#each tags as tag (tag)}
        <button
          aria-label={tag}
          title={tag}
          class:active={selected === tag}
          onclick={() => onOpen?.({ name: tag, path: "", kind: "tag" })}
          ><i
            class:gray={tag === "Gray"}
            style:background-color={tag === "Gray" ? undefined : tag.toLowerCase()}></i
          ><span>{tag}</span></button>
      {/each}
    </nav>
  </Sidebar.Content>
  <Sidebar.Footer class="hidden" />
  <Sidebar.Rail onResizeStart={resizeSidebar} />
</Sidebar.Root>
