<script lang="ts" module>
  import Clock3Icon from "@lucide/svelte/icons/clock-3";
  import CloudIcon from "@lucide/svelte/icons/cloud";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import GlobeIcon from "@lucide/svelte/icons/globe";
  import HardDriveIcon from "@lucide/svelte/icons/hard-drive";
  import LinuxIcon from "./linux-icon.svelte";
  import NetworkIcon from "@lucide/svelte/icons/network";
  import ServerIcon from "@lucide/svelte/icons/server";
  import HouseIcon from "@lucide/svelte/icons/house";
  import UsersIcon from "@lucide/svelte/icons/users";

  import { OVERVIEW, sameLocation, type Location } from "#lib/tabs/tabs.js";
  export type { Location };
</script>

<script lang="ts">
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as Sidebar from "#lib/components/ui/sidebar/index.js";
  import ClipboardCopyIcon from "@lucide/svelte/icons/clipboard-copy";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import EjectIcon from "@lucide/svelte/icons/eject";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import LockIcon from "@lucide/svelte/icons/lock";
  import PanelLeftIcon from "@lucide/svelte/icons/panel-left";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import StarIcon from "@lucide/svelte/icons/star";
  import StarOffIcon from "@lucide/svelte/icons/star-off";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import { invoke } from "@tauri-apps/api/core";
  import { formatSize } from "#lib/components/custom/preview/format.js";
  import { diskSpace } from "#lib/system/disk-space.svelte.js";
  import { drag } from "#lib/file-drag/drag.svelte.js";
  import { canFavorite, favoriteDropIndexAt, moveTo } from "#lib/favorites/favorites.js";
  import { trackPointerDrag } from "#lib/file-drag/pointer-drag.js";
  import { atWindowEdge, startNativeDrag } from "#lib/file-drag/native-drag.js";
  import { isNetworkProtocol } from "#lib/remote/network-locations.js";
  import { sidebarLocations, volumeLocations } from "./sidebar-sections.js";
  import type { LocationStatus } from "#lib/remote/network-status.svelte.js";
  import { onMount, tick, type ComponentProps } from "svelte";

  let {
    ref = $bindable(null),
    selected = OVERVIEW,
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
    onEjectLocation,
    onAddFavorite,
    onRemoveFavorite,
    onReorderFavorites,
    statuses = {},
    brand = false,
    ...restProps
  }: ComponentProps<typeof Sidebar.Root> & {
    selected?: Location;
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
    onEjectLocation?: (location: Location) => void;
    /** Adds a local folder to the favorites at `index`. */
    onAddFavorite?: (path: string, index: number) => void;
    onRemoveFavorite?: (location: Location) => void;
    onReorderFavorites?: (paths: string[]) => void;
    /** Connection state of network locations, by path. */
    statuses?: Record<string, LocationStatus>;
    /** Show the logo, name and sidebar toggle in the header (Windows and Linux). */
    brand?: boolean;
  } = $props();

  const FAVORITES_OPEN_KEY = "sidebar-favorites-open";
  const VOLUMES_OPEN_KEY = "sidebar-volumes-open";
  const LOCATIONS_OPEN_KEY = "sidebar-locations-open";
  let favoritesOpen = $state(true);
  let volumesOpen = $state(true);
  let locationsOpen = $state(true);
  let sectionsReady = $state(false);

  onMount(() => {
    favoritesOpen = localStorage.getItem(FAVORITES_OPEN_KEY) !== "false";
    volumesOpen = localStorage.getItem(VOLUMES_OPEN_KEY) !== "false";
    locationsOpen = localStorage.getItem(LOCATIONS_OPEN_KEY) !== "false";
    const frame = requestAnimationFrame(() => (sectionsReady = true));
    return () => cancelAnimationFrame(frame);
  });

  function toggleFavorites() {
    favoritesOpen = !favoritesOpen;
    localStorage.setItem(FAVORITES_OPEN_KEY, String(favoritesOpen));
  }

  function toggleLocations() {
    locationsOpen = !locationsOpen;
    localStorage.setItem(LOCATIONS_OPEN_KEY, String(locationsOpen));
  }

  function toggleVolumes() {
    volumesOpen = !volumesOpen;
    localStorage.setItem(VOLUMES_OPEN_KEY, String(volumesOpen));
  }

  let volumes = $derived(volumeLocations(locations));
  let savedLocations = $derived(sidebarLocations(locations));

  // Volumes show how full they are; the reading is shared with the status bar.
  $effect(() => {
    if (volumes.length) void diskSpace.refresh();
  });

  /** Short protocol label for a saved remote location, like `SMB` or `S3`. */
  function protocolLabel(kind: string): string {
    return kind === "webdav" ? "DAV" : kind.toUpperCase();
  }

  const QUICK_PLACES: { kind: "overview" | "recents" | "shared"; name: string; icon: typeof GaugeIcon }[] = [
    { kind: "overview", name: "Overview", icon: GaugeIcon },
    { kind: "recents", name: "Recents", icon: Clock3Icon },
    { kind: "shared", name: "Shared", icon: UsersIcon },
  ];

  let trashBytes = $state<number | null>(null);
  let trashReadAt = 0;

  /** Trash size, read on start and again when the window regains focus after a minute. */
  function readTrash() {
    if (Date.now() - trashReadAt < 60_000) return;
    trashReadAt = Date.now();
    invoke<number | null>("trash_usage")
      .then((bytes) => (trashBytes = bytes))
      .catch(() => {});
  }

  onMount(() => {
    readTrash();
    window.addEventListener("focus", readTrash);
    return () => window.removeEventListener("focus", readTrash);
  });

  let externalDroppable = $derived(
    drag.entry !== null &&
      drag.entries.length <= 1 &&
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
          startNativeDrag([favorite.path], favorite.name, "folder");
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

  let sourcesNav = $state<HTMLElement | null>(null);
  let lastFocused: HTMLElement | null = null;

  function sidebarItems(): HTMLElement[] {
    return sourcesNav
      ? [...sourcesNav.querySelectorAll<HTMLElement>("[data-sidebar-item]")].filter((item) => !item.closest("[inert]"))
      : [];
  }

  /** One sidebar item is in the Tab order at a time; arrows or j/k move between them. */
  function rove(current: HTMLElement) {
    for (const item of sidebarItems()) item.tabIndex = item === current ? 0 : -1;
  }

  function handleFocusIn(event: FocusEvent) {
    const item = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-sidebar-item]") : null;
    if (!item) return;
    lastFocused = item;
    rove(item);
  }

  $effect(() => {
    void favorites.length;
    void locations.length;
    void volumesOpen;
    void favoritesOpen;
    void locationsOpen;
    void tick().then(() => {
      const items = sidebarItems();
      const current = (lastFocused && items.includes(lastFocused) ? lastFocused : null) ?? items[0];
      if (current) rove(current);
    });
  });

  /** Focuses the item focused last, else the active one, else the first. */
  export function focusItem(): boolean {
    const items = sidebarItems();
    const item =
      (lastFocused && items.includes(lastFocused) ? lastFocused : null) ??
      items.find((candidate) => candidate.classList.contains("active")) ??
      items[0];
    if (!item) return false;
    item.focus();
    return true;
  }

  export function moveFocus(delta: 1 | -1): boolean {
    const items = sidebarItems();
    const index = items.findIndex((item) => item === document.activeElement);
    if (index < 0) return focusItem();
    items[Math.max(0, Math.min(items.length - 1, index + delta))].focus();
    return true;
  }

  export function openFocused(): boolean {
    const item =
      document.activeElement instanceof HTMLElement ? document.activeElement.closest<HTMLElement>("[data-sidebar-item]") : null;
    if (!item) return false;
    item.click();
    return true;
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
    <nav class="finder-sources" aria-label="Finder sidebar" data-key-scope="sidebar" bind:this={sourcesNav} onfocusin={handleFocusIn}>
      <div class="quick-places">
        {#each QUICK_PLACES as place (place.kind)}
          <button
            class="quick-place"
            data-sidebar-item
            aria-label={place.name}
            title={place.name}
            class:active={selected.kind === place.kind}
            onclick={() => onOpen?.({ name: place.name, path: "", kind: place.kind })}><place.icon /><span>{place.name}</span></button>
        {/each}
      </div>
      <div class="finder-section">
        <button
          type="button"
          class="section-toggle"
          data-sidebar-item
          aria-expanded={favoritesOpen}
          aria-controls="sidebar-favorites"
          onclick={toggleFavorites}><ChevronDownIcon class={favoritesOpen ? "" : "closed"} /><span>Favorites</span><small class="section-count">{favorites.length}</small></button>
      </div>
      <div id="sidebar-favorites" class="section-collapse" class:collapsed={!favoritesOpen} class:ready={sectionsReady} inert={!favoritesOpen}>
        <div class="section-collapse-inner">
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
                    aria-label={favorite.name} data-sidebar-item
                    title={favorite.path}
                    class:active={sameLocation(selected, favorite)}
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
        </div>
      </div>
      {#if volumes.length}
        <div class="finder-section">
          <button
            type="button"
            class="section-toggle"
            data-sidebar-item
            aria-expanded={volumesOpen}
            aria-controls="sidebar-volumes"
            onclick={toggleVolumes}><ChevronDownIcon class={volumesOpen ? "" : "closed"} /><span>Volumes</span><small class="section-count">{volumes.length}</small></button>
        </div>
        <div id="sidebar-volumes" class="section-collapse" class:collapsed={!volumesOpen} class:ready={sectionsReady} inert={!volumesOpen}>
          <div class="section-collapse-inner">
            {#each volumes as location (location.path)}
              {@const volume = diskSpace.volumeFor(location.path)}
              {@const used = volume && volume.total_bytes > 0 ? 1 - volume.free_bytes / volume.total_bytes : null}
              <div class="location-row volume-row pb-2" class:has-trail={location.kind === "hfs-volume"}>
                <button
                  aria-label={location.name} data-sidebar-item
                  title={volume ? `${location.path} · ${formatSize(volume.free_bytes)} free of ${formatSize(volume.total_bytes)}` : location.path}
                  class:active={sameLocation(selected, location)}
                  onclick={() => onOpen?.(location)}>{#if location.kind === "wsl-volume"}<LinuxIcon />{:else}<HardDriveIcon />{/if}<span>{location.name}</span>{#if volume}<small class="row-meta">{formatSize(volume.free_bytes)}</small>{/if}</button>
                {#if used !== null}
                  <span class="volume-meter" class:full={used > 0.9} aria-hidden="true"><span style:width={`${Math.round(used * 100)}%`}></span></span>
                {/if}
                {#if location.kind === "hfs-volume"}
                  <button
                    class="location-trail"
                    aria-label={`Eject ${location.name}`}
                    title="Eject"
                    onclick={() => onEjectLocation?.(location)}><EjectIcon /></button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
      <div class="finder-section">
        <button
          type="button"
          class="section-toggle"
          data-sidebar-item
          aria-expanded={locationsOpen}
          aria-controls="sidebar-locations"
          onclick={toggleLocations}><ChevronDownIcon class={locationsOpen ? "" : "closed"} /><span>Locations</span><small class="section-count">{savedLocations.length}</small></button>
        {#if onAddLocation}
          <button
            class="section-add"
            aria-label="Add location"
            title="Add location"
            onclick={() => onAddLocation()}><PlusIcon /></button>
        {/if}
      </div>
      <div id="sidebar-locations" class="section-collapse" class:collapsed={!locationsOpen} class:ready={sectionsReady} inert={!locationsOpen}>
        <div class="section-collapse-inner">
      {#each savedLocations as location (location.path)}
        {#if location.kind === "s3" || isNetworkProtocol(location.kind)}
          {@const status = statuses[location.path]}
          <ContextMenu.Root>
            <ContextMenu.Trigger>
              {#snippet child({ props })}
                <div {...props} class="location-row" data-state={status?.state ?? "idle"}>
                  <button
                    aria-label={location.name} data-sidebar-item
                    title={status?.state === "locked" ? `${location.name}: password needed` : location.name}
                    class:active={sameLocation(selected, location)}
                    aria-busy={status?.state === "connecting"}
                    onclick={() => onOpen?.(location)}
                    >{#if status?.state === "connecting"}<LoaderCircleIcon class="location-spinner" />{:else if location.kind === "s3"}<CloudIcon />{:else if location.kind === "webdav"}<GlobeIcon />{:else if location.kind === "sftp" || location.kind === "ftp"}<ServerIcon />{:else}<NetworkIcon />{/if}<span
                      >{location.name}</span
                    >{#if status?.state !== "error" && status?.state !== "locked"}<small class="row-badge">{protocolLabel(location.kind)}</small>{/if}</button>
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
            aria-label={location.name} data-sidebar-item
            title={location.path}
            class:active={sameLocation(selected, location)}
            onclick={() => onOpen?.(location)}
            >{#if location.kind === "home"}<HouseIcon />{:else}<HardDriveIcon />{/if}<span>{location.name}</span></button>
        {/if}
      {/each}
        </div>
      </div>
    </nav>
  </Sidebar.Content>
  <div class="sidebar-footer">
    <button class="sidebar-footer-trash" title="Open the Trash" onclick={() => void invoke("open_trash").catch(() => {})}
      ><Trash2Icon /><span>Trash</span>{#if trashBytes}<small class="row-meta">{formatSize(trashBytes)}</small>{/if}</button>
    <button class="sidebar-footer-settings" aria-label="Settings" title="Settings" onclick={() => void invoke("open_settings").catch(() => {})}
      ><SettingsIcon /></button>
  </div>
  <Sidebar.Rail onResizeStart={resizeSidebar} />
</Sidebar.Root>
