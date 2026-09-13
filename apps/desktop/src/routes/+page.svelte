<script lang="ts">
  import { dev } from "$app/environment";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { devToolsVisible } from "$lib/stores/devTools";
  import { onMount, untrack } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
  import ArrowRightIcon from "@lucide/svelte/icons/arrow-right";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import Columns2Icon from "@lucide/svelte/icons/columns-2";
  import Grid2X2Icon from "@lucide/svelte/icons/grid-2x2";
  import ListIcon from "@lucide/svelte/icons/list";
  import PanelLeftIcon from "@lucide/svelte/icons/panel-left";
  import PanelRightCloseIcon from "@lucide/svelte/icons/panel-right-close";
  import PanelRightOpenIcon from "@lucide/svelte/icons/panel-right-open";
  import Rows2Icon from "@lucide/svelte/icons/rows-2";
  import SearchIcon from "@lucide/svelte/icons/search";
  import AppSidebar from "$lib/components/custom/sidebar/app-sidebar.svelte";
  import CommandPalette from "$lib/components/custom/command-palette.svelte";
  import { openCommandPalette } from "$lib/state/command-palette.svelte";
  import AddLocationDialog from "$lib/components/custom/sidebar/add-location/add-location-dialog.svelte";
  import PasswordPrompt from "$lib/components/custom/sidebar/add-location/password-prompt.svelte";
  import NewBucketDialog from "$lib/components/custom/remote/new-bucket-dialog.svelte";
  import BucketSettingsDialog from "$lib/components/custom/remote/bucket-settings-dialog.svelte";
  import { ConfirmHost, confirmation } from "$lib/components/custom/dialog/index.js";
  import { FilePane } from "$lib/file-pane/index.js";
  import { FilePaneController } from "$lib/file-pane/controller.svelte.js";
  import { PanesStore } from "$lib/panes/panes.svelte.js";
  import { removeRemoteLocation } from "$lib/remote/remote-locations.js";
  import { formatLocationUrl } from "$lib/remote/location-url.js";
  import {
    describeError,
    getNetworkLocation,
    isNetworkPath,
    isNetworkProtocol,
    removeNetworkLocation,
    toConnectError,
    trustNetworkHost,
    type ConnectError,
    type TrustRequest,
  } from "$lib/remote/network-locations.js";
  import { remapMountPath } from "$lib/remote/network-paths.js";
  import { networkStatus, type ConnectOutcome } from "$lib/remote/network-status.svelte.js";
  import { copyItem, moveItem } from "$lib/file-ops/files.js";
  import { message } from "@tauri-apps/plugin-dialog";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/index.js";
  import ActivityDrawer from "$lib/components/custom/activity/activity-drawer.svelte";
  import { JobsStore } from "$lib/transfers/jobs.svelte.js";
  import { isRunning, type TransferEventPayload } from "$lib/transfers/jobs.js";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import { OVERVIEW, type Location } from "$lib/tabs/tabs.js";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import UpdateBanner from "$lib/updates/update-banner.svelte";
  import { updates } from "$lib/updates/updates.svelte.js";

  const panes = new PanesStore();
  const controllers = new Map<string, FilePaneController>();

  function controllerFor(paneId: string): FilePaneController {
    let controller = controllers.get(paneId);
    if (!controller) {
      const pane = panes.panes.find((candidate) => candidate.id === paneId);
      const tabs = pane?.tabs ?? panes.activeTabs;
      controller = new FilePaneController(paneId, tabs, () => void getCurrentWindow().close());
      const created = controller;
      created.openShare = (path, options) => openShare(created, path, options);
      controllers.set(paneId, controller);
    }
    return controller;
  }

  let activeController = $derived(controllerFor(panes.activeId));

  let sidebarFloating = $state(false);
  let sidebarOpen = $state(true);
  let sidebarResizing = $state(false);
  let isLinux = $state(false);
  let platform = $state<"macos" | "windows" | "linux" | "unknown">("unknown");
  let revealLabel = $derived(
    platform === "macos"
      ? "Show in Finder"
      : platform === "windows"
        ? "Show in File Explorer"
        : "Show in File Manager",
  );
  let favorites = $state<Location[]>([]);
  let locations = $state<Location[]>([]);
  let addLocationOpen = $state(false);
  let editingLocation = $state<Location | null>(null);
  let passwordLocation = $state<Location | null>(null);
  let passwordOpen = $state(false);
  let passwordRequest: ConnectRequest | null = null;
  let newBucketOpen = $state(false);
  let bucketSettingsOpen = $state(false);
  let bucketSettingsTarget = $state<DirectoryEntry | null>(null);
  let showHiddenFiles = $state(false);
  let showFps = $state(false);
  let activityOpen = $state(false);
  const jobs = new JobsStore();
  let transferActivity = $derived(
    (() => {
      const active = jobs.jobs.find(isRunning);
      return active ? `${active.kind} ${active.label || active.destination}…` : null;
    })(),
  );

  function addedLocation(location: Location) {
    locations = [...locations, location];
    openAnyLocation(location);
  }

  /** Opens the local folder of a mounted network share. */
  function openMounted(location: Location, mountPath: string, options: { newTab?: boolean } = {}) {
    activeController.openLocation({ name: location.name, path: mountPath, kind: "folder" }, options);
  }

  type Credentials = { password?: string; remember?: boolean };

  /** A connection attempt that can ask for a password or for trust, then try again. */
  type ConnectRequest = {
    location: Location;
    attempt: (credentials: Credentials) => Promise<ConnectOutcome>;
    onConnected: (mountPath: string) => void;
    /** Failures not already shown on the sidebar row. */
    onFailed?: (error: ConnectError) => void;
  };

  async function runConnect(request: ConnectRequest) {
    handleOutcome(request, await request.attempt({}));
  }

  /** Network locations connect first (mounting SMB, NFS and WebDAV, signing in to SFTP and FTP). */
  function openAnyLocation(location: Location, options: { newTab?: boolean } = {}) {
    if (!isNetworkPath(location.path) || !isNetworkProtocol(location.kind)) {
      activeController.openLocation(location, options);
      return;
    }
    void runConnect({
      location,
      attempt: (credentials) => networkStatus.connect(location, credentials),
      onConnected: (mountPath) => openMounted(location, mountPath, options),
    });
  }

  /** Mounts a share listed by an SMB server location, then opens it in the same pane. */
  function openShare(controller: FilePaneController, path: string, options: { newTab?: boolean }) {
    const location = networkStatus.ownerOf(path);
    if (!location) return;
    const name = path.split("/").filter(Boolean).pop() ?? location.name;
    void runConnect({
      location,
      attempt: (credentials) => networkStatus.mountShare(location, path, credentials),
      onConnected: (mountPath) => controller.openLocation({ name, path: mountPath, kind: "folder" }, options),
      onFailed: (error) => void message(describeError(error), { title: `Couldn’t open “${name}”`, kind: "error" }),
    });
  }

  /** Connects a location again from a pane showing it as disconnected, and returns to the same folder. */
  function reconnect(controller: FilePaneController, location: Location) {
    const current = controller.listingPath;
    const reopen = (from: string | null, mountPath: string) => {
      const next = from && !isNetworkPath(mountPath) ? remapMountPath(current, from, mountPath) : current;
      if (next === current) controller.reload();
      else controller.openLocation({ ...controller.tabs.active.location, path: next });
    };
    const onFailed = (error: ConnectError) =>
      void message(describeError(error), { title: `Couldn’t reconnect “${location.name}”`, kind: "error" });
    const share = networkStatus.shareAt(location, current);
    if (share) {
      const sharePath = `${location.path.replace(/\/+$/, "")}/${share.name}`;
      void runConnect({
        location,
        attempt: (credentials) => networkStatus.mountShare(location, sharePath, credentials),
        onConnected: (mountPath) => reopen(share.path, mountPath),
        onFailed,
      });
      return;
    }
    const previous = networkStatus.lastMount(location);
    void runConnect({
      location,
      attempt: (credentials) => networkStatus.connect(location, credentials),
      onConnected: (mountPath) => reopen(previous, mountPath),
      onFailed,
    });
  }

  function handleOutcome(request: ConnectRequest, outcome: ConnectOutcome) {
    if (outcome.kind === "connected") {
      request.onConnected(outcome.mountPath);
    } else if (outcome.kind === "password") {
      passwordRequest = request;
      passwordLocation = request.location;
      passwordOpen = true;
    } else if (outcome.kind === "trust") {
      askTrust(request, outcome.error.trust, outcome.error.message);
    } else {
      request.onFailed?.(outcome.error);
    }
  }

  /** Asks before pinning a server key or certificate, then connects again. */
  function askTrust(request: ConnectRequest, trust: TrustRequest, reason: string) {
    const what = trust.algorithm === "certificate" ? "certificate" : "host key";
    confirmation.ask({
      title: trust.changed ? `Trust the new ${what} of “${trust.host}”?` : `Trust “${trust.host}”?`,
      description: `${reason} ${trust.algorithm === "certificate" ? "Certificate" : trust.algorithm} fingerprint: ${trust.fingerprint}`,
      confirmLabel: trust.changed ? `Trust new ${what}` : "Trust and connect",
      pendingLabel: "Connecting…",
      variant: trust.changed ? "destructive" : "primary",
      onconfirm: async () => {
        await trustNetworkHost(trust).catch((error) => {
          throw new Error(describeError(toConnectError(error)));
        });
        const outcome = await request.attempt({});
        if (outcome.kind === "failed") throw new Error(describeError(outcome.error));
        queueMicrotask(() => handleOutcome(request, outcome));
      },
    });
  }

  async function submitPassword(password: string, remember: boolean): Promise<string | null> {
    const request = passwordRequest;
    if (!request) return null;
    const outcome = await request.attempt({ password, remember });
    if (outcome.kind === "connected") {
      request.onConnected(outcome.mountPath);
      return null;
    }
    if (outcome.kind === "trust") {
      askTrust(request, outcome.error.trust, outcome.error.message);
      return null;
    }
    return describeError(outcome.error);
  }

  async function disconnectLocation(location: Location) {
    const status = networkStatus.status(location.path);
    try {
      await networkStatus.disconnect(location);
    } catch (error) {
      await message(describeError(toConnectError(error)), { title: "Couldn’t disconnect", kind: "error" });
      return;
    }
    if (status.state !== "connected") return;
    for (const controller of controllers.values()) {
      if (networkStatus.ownerOf(controller.listingPath)?.path === location.path) controller.openLocation(OVERVIEW);
    }
  }

  function removeLocation(location: Location) {
    confirmation.ask({
      title: `Remove “${location.name}”?`,
      description: "The connection and its saved secret are removed from this computer. Files on the server stay as they are.",
      confirmLabel: "Remove",
      pendingLabel: "Removing…",
      onconfirm: async () => {
        if (isNetworkPath(location.path)) {
          await removeNetworkLocation(location).catch((error) => {
            throw new Error(describeError(toConnectError(error)));
          });
        } else {
          await removeRemoteLocation(location);
        }
        locations = locations.filter((candidate) => candidate.path !== location.path);
        networkStatus.forget(location.path);
        if (activeController.listingPath.startsWith(location.path)) activeController.openLocation(OVERVIEW);
      },
    });
  }

  function editLocation(location: Location) {
    editingLocation = location;
    addLocationOpen = true;
  }

  function updatedLocation(previousPath: string, location: Location) {
    locations = locations.map((candidate) => (candidate.path === previousPath ? location : candidate));
    // A changed path points at another share, so the old mount state no longer applies.
    if (previousPath !== location.path) networkStatus.forget(previousPath);
    networkStatus.invalidate(location.path);
  }

  function copyLocationAddress(location: Location) {
    // A promised ClipboardItem keeps the click's user activation while the address loads.
    const text = getNetworkLocation(location.path).then(
      (input) => new Blob([formatLocationUrl(input)], { type: "text/plain" }),
    );
    const copied =
      typeof ClipboardItem === "undefined"
        ? text.then((blob) => blob.text()).then((value) => navigator.clipboard.writeText(value))
        : navigator.clipboard.write([new ClipboardItem({ "text/plain": text })]);
    copied.catch((error) => {
      void message(describeError(toConnectError(error)), { title: "Couldn’t copy address", kind: "error" });
    });
  }

  $effect(() => {
    if (!addLocationOpen) editingLocation = null;
  });

  $effect(() => {
    networkStatus.locations = locations;
  });

  function openBucketSettings(entry: DirectoryEntry) {
    bucketSettingsTarget = entry;
    bucketSettingsOpen = true;
  }

  async function handleExternalDrop(targetPaneId: string, path: string, options: { move: boolean }) {
    const target = controllerFor(targetPaneId);
    const destination = target.listingPath;
    if (!destination || target.remoteListing) return;
    try {
      if (options.move) await moveItem(path, destination);
      else await copyItem(path, destination);
    } catch (error) {
      await message(error instanceof Error ? error.message : String(error), {
        title: "Couldn’t transfer",
        kind: "error",
      });
    }
    for (const controller of controllers.values()) void controller.refreshListing(controller.listingPath);
  }

  let sidebarWidth = $state(192);
  const sidebarDefaultWidth = 192;
  const sidebarMinimumWidth = 192;
  const sidebarMaximumWidth = sidebarDefaultWidth * 2;
  type ViewTransitionDocument = Document & {
    startViewTransition?: (update: () => void) => unknown;
  };

  onMount(() => {
    void invoke<string>("os_detection").then((detected) => {
      platform = detected as typeof platform;
      isLinux = detected === "linux";
    });
    void invoke<Location[]>("favorites").then((nativeFavorites) => (favorites = nativeFavorites));
    void invoke<Location[]>("locations").then((savedLocations) => {
      locations = savedLocations;
      void networkStatus.refresh().catch(() => {});
    });
    sidebarFloating = localStorage.getItem("sidebar-floating") === "true";
    const savedWidth = Number(localStorage.getItem("sidebar-width"));
    if (savedWidth >= sidebarMinimumWidth) {
      sidebarWidth = Math.min(savedWidth, sidebarMaximumWidth);
    }
    showHiddenFiles = localStorage.getItem("show-hidden-files") === "true";
    void invoke("set_sidebar_floating", { floating: sidebarFloating });
    void invoke("set_show_hidden_files", { show: showHiddenFiles });
    const savedShowFps = localStorage.getItem("show-fps");
    showFps = savedShowFps === null ? dev : savedShowFps === "true";
    void invoke("set_show_fps", { show: showFps });
    const unlisteners = [
      listen<TransferEventPayload>("transfer-progress", ({ payload }) => jobs.upsert(payload)),
      getCurrentWebview().onDragDropEvent(({ payload }) => {
        const controller = activeController;
        const writable = controller.remoteListing && !controller.remoteRoot;
        if (payload.type === "enter") controller.remoteDropActive = writable && payload.paths.length > 0;
        else if (payload.type === "leave") controller.remoteDropActive = false;
        else if (payload.type === "drop") {
          controller.remoteDropActive = false;
          if (writable && payload.paths.length) void controller.uploadToListing(payload.paths);
        }
      }),
      listen<boolean>("sidebar-floating", ({ payload }) => {
        sidebarFloating = payload;
        localStorage.setItem("sidebar-floating", String(payload));
      }),
      listen<boolean>("show-hidden-files", ({ payload }) => {
        showHiddenFiles = payload;
        localStorage.setItem("show-hidden-files", String(payload));
      }),
      listen<boolean>("show-fps", ({ payload }) => {
        showFps = payload;
        localStorage.setItem("show-fps", String(payload));
      }),
      listen<boolean>("dev-tools", ({ payload }) => {
        devToolsVisible.set(payload);
        localStorage.setItem("dev-tools", String(payload));
      }),
      listen<{ path: string; sizes: { path: string; size: number }[] }>(
        "directory-sizes",
        ({ payload }) => {
          for (const controller of controllers.values()) controller.applyDirectorySizes(payload);
        },
      ),
      listen("request-create-folder", () => void activeController.createItem("folder")),
      listen("request-create-file", () => void activeController.createItem("file")),
      listen("request-open", (event) => void activeController.openPath(event.payload as string)),
      listen("tab-new", () => panes.newTab()),
      listen("tab-new-other", () => panes.newTabInOtherPane()),
      listen("tab-close", () => panes.panes.length > 1 ? panes.closeActivePane() : activeController.closeTab(activeController.tabs.activeId)),
      listen("tab-next", () => activeController.tabs.cycle(1)),
      listen("tab-prev", () => activeController.tabs.cycle(-1)),
      listen("pane-toggle", () => panes.togglePane()),
      listen("pane-orientation", () => panes.toggleLayout()),
      listen("command-palette", () => openCommandPalette()),
      listen("check-for-updates", () => void updates.check({ manual: true })),
    ];
    // Development builds have no published release to compare against.
    const stopUpdates = dev ? () => {} : updates.start();
    return () => {
      stopUpdates();
      unlisteners.forEach((unlisten) => void unlisten.then((stop) => stop()));
    };
  });

  // Load each pane's active tab whenever its location or active tab changes. `locationKey` is a
  // stable string so selection/view updates (which replace the active tab object) don't reload.
  $effect(() => {
    for (const pane of panes.panes) {
      const controller = controllerFor(pane.id);
      void pane.tabs.locationKey;
      untrack(() => void controller.loadLocation(pane.tabs.active.location));
    }
  });

  // The menu's Open item follows the active pane's selection.
  $effect(() => {
    const controller = activeController;
    const entry = controller.entries.find((candidate) => candidate.path === controller.selectedEntryPath);
    void invoke("set_open_target", {
      path: entry?.path ?? "",
      isDirectory: entry?.is_directory ?? false,
      enabled: Boolean(entry),
    });
  });

  function resizeSidebar(width: number) {
    sidebarWidth = Math.max(
      sidebarMinimumWidth,
      Math.min(sidebarMaximumWidth, width, window.innerWidth - sidebarMinimumWidth),
    );
    sidebarOpen = true;
    localStorage.setItem("sidebar-width", String(sidebarWidth));
  }

  /** Enter opens the active pane's selection, unless focus is in a text field, menu or control. */
  function handleEnterKeydown(event: KeyboardEvent) {
    activeController.handleEnterKeydown(event);
  }

  function handleTabKeydown(event: KeyboardEvent) {
    const modifier = isLinux ? event.ctrlKey : event.metaKey;
    if (!modifier || event.altKey) return;
    if (event.shiftKey && event.key.toLowerCase() === "t") {
      event.preventDefault();
      panes.newTabInOtherPane();
      return;
    }
    if (!event.shiftKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      const index = panes.panes.findIndex((pane) => pane.id === panes.activeId);
      const target = panes.panes[index + (event.key === "ArrowLeft" ? -1 : 1)];
      if (target) {
        event.preventDefault();
        panes.setActive(target.id);
      }
      return;
    }
    if (event.shiftKey || !/^[1-9]$/.test(event.key)) return;
    if (panes.activeTabs.selectDigit(Number(event.key))) event.preventDefault();
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    handleTabKeydown(event);
    handleEnterKeydown(event);
    activeController.handleFileListKeydown(event, showHiddenFiles);
  }

  function toggleSidebar() {
    withSidebarViewTransition(() => {
      if (sidebarOpen) {
        sidebarOpen = false;
        return;
      }
      sidebarWidth = sidebarDefaultWidth;
      sidebarOpen = true;
      localStorage.setItem("sidebar-width", String(sidebarWidth));
    });
  }

  function withSidebarViewTransition(update: () => void) {
    const documentWithViewTransition = document as ViewTransitionDocument;
    if (documentWithViewTransition.startViewTransition) {
      documentWithViewTransition.startViewTransition(update);
      return;
    }
    update();
  }
</script>

<svelte:head><title>{activeController.selected} - Lite Explorer</title></svelte:head>
<svelte:window onkeydown={handleWindowKeydown} />

<Sidebar.Provider
  bind:open={sidebarOpen}
  onToggle={toggleSidebar}
  style={`--sidebar-width: ${sidebarWidth}px;`}
  class={`finder-window${isLinux ? " platform-linux" : ""}${sidebarFloating ? " sidebar-floating" : ""}${sidebarResizing ? " sidebar-resizing" : ""}${sidebarOpen ? "" : " sidebar-collapsed"}`}>
  <AppSidebar
    selected={activeController.selected}
    {favorites}
    {locations}
    onOpen={(location) => openAnyLocation(location)}
    onAddLocation={() => (addLocationOpen = true)}
    onRemoveLocation={removeLocation}
    onEditLocation={editLocation}
    onCopyLocationAddress={copyLocationAddress}
    onDisconnectLocation={(location) => void disconnectLocation(location)}
    statuses={networkStatus.statuses}
    open={sidebarOpen}
    width={sidebarWidth}
    onResize={resizeSidebar}
    onResizeStart={() => (sidebarResizing = true)}
    onResizeEnd={() => (sidebarResizing = false)}
    onToggle={toggleSidebar}
    variant={sidebarFloating ? "floating" : "sidebar"}
    onOpenPalette={openCommandPalette} />
  {#if !isLinux}
    <button
      class="sidebar-trigger"
      class:sidebar-trigger-open={sidebarOpen}
      aria-label="Toggle sidebar"
      title="Toggle sidebar"
      onclick={toggleSidebar}><PanelLeftIcon /></button>
  {/if}
  <AddLocationDialog
    bind:open={addLocationOpen}
    editing={editingLocation}
    onAdded={addedLocation}
    onUpdated={updatedLocation} />
  <PasswordPrompt bind:open={passwordOpen} location={passwordLocation} onSubmit={submitPassword} />
  <NewBucketDialog
    bind:open={newBucketOpen}
    location={activeController.listingPath}
    onCreated={(result) => activeController.openCreatedBucket(result)} />
  {#if bucketSettingsTarget}
    <BucketSettingsDialog
      bind:open={bucketSettingsOpen}
      path={bucketSettingsTarget.path}
      name={bucketSettingsTarget.name} />
  {/if}
  <ConfirmHost />
  <UpdateBanner />
  <CommandPalette {favorites} {locations} recents={activeController.recents} onNavigate={(location, opts) => openAnyLocation(location, opts)} />
  <Sidebar.Inset class="finder-content">
    <header class="finder-toolbar" data-tauri-drag-region="deep">
      <div class="toolbar-leading">
        {#if isLinux}
          <button
            class="sidebar-trigger sidebar-trigger-linux"
            aria-label="Toggle sidebar"
            title="Toggle sidebar"
            onclick={toggleSidebar}><PanelLeftIcon /></button>
        {/if}
        {#if activeController.selected !== "Overview"}
          <div class="toolbar-controls">
            <button aria-label="Back" disabled={!activeController.canBack} onclick={() => activeController.goBack()}><ArrowLeftIcon /></button>
            <button aria-label="Forward" disabled={!activeController.canForward} onclick={() => activeController.goForward()}><ArrowRightIcon /></button>
          </div>
        {/if}
      </div>
      <button class="location-title" aria-label="Current location"
        >{activeController.selected}<ChevronDownIcon /></button>
      <div class="toolbar-actions">
        {#if activeController.selected !== "Overview"}
          <button aria-label="List view" aria-pressed={activeController.viewMode === "list"} onclick={() => activeController.tabs.update({ viewMode: "list" })}><ListIcon /></button>
          <button aria-label="Icon view" aria-pressed={activeController.viewMode === "grid"} onclick={() => activeController.tabs.update({ viewMode: "grid" })}><Grid2X2Icon /></button>
          <button
            aria-label={activeController.previewOpen ? "Hide preview" : "Show preview"}
            aria-pressed={activeController.previewOpen}
            title={activeController.previewOpen ? "Hide preview" : "Show preview"}
            onclick={() => (activeController.previewOpen = !activeController.previewOpen)}>{#if activeController.previewOpen}<PanelRightCloseIcon />{:else}<PanelRightOpenIcon />{/if}</button>
         {/if}
        <button
          aria-label="Toggle second pane"
          aria-pressed={panes.panes.length > 1}
          title="Show Second Pane"
          onclick={() => panes.togglePane()}><Columns2Icon /></button>
        <button
          aria-label="Toggle pane orientation"
          title="Split Horizontally/Vertically"
          onclick={() => panes.toggleLayout()}><Rows2Icon /></button>
        <button
          aria-label="Activity"
          aria-pressed={activityOpen}
          title="Activity"
          class="activity-toggle"
          onclick={() => (activityOpen = !activityOpen)}>
          <RotateCwIcon />
          {#if jobs.activeCount > 0}<span class="activity-badge">{jobs.activeCount}</span>{/if}
        </button>
        <label class="finder-search"
          ><SearchIcon /><input aria-label="Search" placeholder="Search" /></label>
      </div>
    </header>

    <div class="panes" data-layout={panes.layout}>
      {#each panes.panes as pane (pane.id)}
        <FilePane
          controller={controllerFor(pane.id)}
          active={pane.id === panes.activeId}
          onActivate={() => panes.setActive(pane.id)}
          {showHiddenFiles}
          {showFps}
          {revealLabel}
          {transferActivity}
          onOpenBucketSettings={openBucketSettings}
          onNewBucket={() => (newBucketOpen = true)}
          onReconnect={(location) => reconnect(controllerFor(pane.id), location)}
          onExternalDrop={(path, options) => handleExternalDrop(pane.id, path, options)}
          onCrossPaneDrop={(from, to, fromIndex, toIndex) => panes.moveTab(from, to, fromIndex, toIndex)} />
      {/each}
    </div>
    <ActivityDrawer {jobs} open={activityOpen} onToggle={() => (activityOpen = !activityOpen)} />
  </Sidebar.Inset>
</Sidebar.Provider>
