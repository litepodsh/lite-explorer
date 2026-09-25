<script lang="ts">
  import { dev } from "$app/environment";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { devToolsVisible } from "$lib/stores/devTools";
  import { onMount, tick, untrack } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
  import ArrowRightIcon from "@lucide/svelte/icons/arrow-right";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import Columns2Icon from "@lucide/svelte/icons/columns-2";
  import Grid2X2Icon from "@lucide/svelte/icons/grid-2x2";
  import ListIcon from "@lucide/svelte/icons/list";
  import ListChecksIcon from "@lucide/svelte/icons/list-checks";
  import PanelLeftIcon from "@lucide/svelte/icons/panel-left";
  import PanelRightCloseIcon from "@lucide/svelte/icons/panel-right-close";
  import PanelRightOpenIcon from "@lucide/svelte/icons/panel-right-open";
  import CalculatorIcon from "@lucide/svelte/icons/calculator";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import Rows2Icon from "@lucide/svelte/icons/rows-2";
  import FinderSearch from "$lib/components/custom/finder-search.svelte";
  import AppSidebar from "$lib/components/custom/sidebar/app-sidebar.svelte";
  import { isVolumeLocation } from "$lib/components/custom/sidebar/sidebar-sections.js";
  import { addFavorite, fetchFavorites, removeFavorite, reorderFavorites } from "$lib/favorites/favorites.js";
  import { DragGhost } from "$lib/components/custom/drag-ghost/index.js";
  import TitleBar from "$lib/components/custom/titlebar/title-bar.svelte";
  import CommandPalette from "$lib/components/custom/command-palette.svelte";
  import { platformState } from "$lib/state/platform.svelte.js";
  import { withTrailingSlash } from "$lib/keyboard/text.js";
  import { commandPaletteState, openCommandPalette, openCommandPaletteWith } from "$lib/state/command-palette.svelte";
  import WelcomeDialog from "$lib/components/custom/analytics/welcome-dialog.svelte";
  import { analytics } from "$lib/analytics/analytics.svelte.js";
  import AddLocationDialog from "$lib/components/custom/sidebar/add-location/add-location-dialog.svelte";
  import PasswordPrompt from "$lib/components/custom/sidebar/add-location/password-prompt.svelte";
  import NewBucketDialog from "$lib/components/custom/remote/new-bucket-dialog.svelte";
  import BucketSettingsDialog from "$lib/components/custom/remote/bucket-settings-dialog.svelte";
  import { ConfirmHost, confirmation } from "$lib/components/custom/dialog/index.js";
  import ConflictHost from "$lib/archive/conflict-host.svelte";
  import { extraction } from "$lib/archive/extraction.svelte.js";
  import { FilePane } from "$lib/file-pane/index.js";
  import { FilePaneController } from "$lib/file-pane/controller.svelte.js";
  import { PanesStore } from "$lib/panes/panes.svelte.js";
  import { downloadRemoteItems, removeRemoteLocation, uploadRemoteFiles } from "$lib/remote/remote-locations.js";
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
  import { baseName, copyItem, copyItems, moveItems, parentPath } from "$lib/file-ops/files.js";
  import { message } from "@tauri-apps/plugin-dialog";
  import type { DirectoryEntry } from "$lib/components/custom/file-list/index.js";
  import type { DraggedEntry } from "$lib/file-drag/drag.svelte.js";
  import ActivityDrawer from "$lib/components/custom/activity/activity-drawer.svelte";
  import TransferToast from "$lib/transfers/transfer-toast.svelte";
  import { applyDownloadProgress, fileDownloads } from "$lib/transfers/download-progress.svelte.js";
  import { JobsStore } from "$lib/transfers/jobs.svelte.js";
  import { activity, isRunning, type TransferEventPayload } from "$lib/transfers/jobs.js";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import ScissorsIcon from "@lucide/svelte/icons/scissors";
  import XIcon from "@lucide/svelte/icons/x";
  import { OVERVIEW, type Location } from "$lib/tabs/tabs.js";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import UpdateBanner from "$lib/updates/update-banner.svelte";
  import ChangelogDialog from "$lib/updates/changelog-dialog.svelte";
  import { updates } from "$lib/updates/updates.svelte.js";
  import { whatsNew } from "$lib/updates/whats-new.svelte.js";
  import { TransferClipboard } from "$lib/transfer-clipboard/queue.svelte.js";
  import { CommandRegistry } from "$lib/keyboard/commands.js";
  import { KeyboardDispatcher } from "$lib/keyboard/dispatcher.svelte.js";
  import { allBindings, appliesTo } from "$lib/keyboard/keymap.js";
  import { standardCommands } from "$lib/keyboard/app-commands.js";
  import { formatToken, normalizeToken, toKeyPlatform } from "$lib/keyboard/keys.js";
  import type { PaletteCommand } from "$lib/components/custom/command-palette.svelte";
  import WhichKey from "$lib/keyboard/which-key.svelte";
  import ShortcutsDialog from "$lib/keyboard/shortcuts-dialog.svelte";
  import type { ShortcutRow } from "$lib/keyboard/shortcuts-filter.js";
  import type { AppContext } from "$lib/keyboard/context.js";
  import type { Scope } from "$lib/keyboard/scope.js";
  import { downloadDir, homeDir } from "@tauri-apps/api/path";
  import { openShortcuts } from "$lib/state/shortcuts-dialog.svelte.js";
  import { navigationCommands } from "$lib/keyboard/navigation-commands.js";
  import { nextRegion, type RegionSlot } from "$lib/keyboard/focus-cycle.js";
  import { scrollPreview } from "$lib/keyboard/preview-scroll.js";
  import { settings } from "$lib/settings/settings.svelte.js";
  import { prefersReducedMotion } from "$lib/swipe/gesture.js";

  // Before any store reads a preference.
  settings.load();
  const panes = new PanesStore();
  const controllers = new Map<string, FilePaneController>();
  const transferClipboard = new TransferClipboard();
  let transferClipboardOpen = $state(false);
  let pasteInProgress = $state(false);
  let pasteJobId: string | null = null;
  let pasteCancelled = false;
  let pastedPaths = $state(new Set<string>());

  function controllerFor(paneId: string): FilePaneController {
    let controller = controllers.get(paneId);
    if (!controller) {
      const pane = panes.panes.find((candidate) => candidate.id === paneId);
      const tabs = pane?.tabs ?? panes.activeTabs;
      controller = new FilePaneController(paneId, tabs, () => void getCurrentWindow().close());
      const created = controller;
      created.openShare = (path, options) => openShare(created, path, options);
      created.onTransferClipboard = (entry) => {
        transferClipboard.append(entry);
        transferClipboardOpen = true;
      };
      controllers.set(paneId, controller);
    }
    return controller;
  }

  let activeController = $derived(controllerFor(panes.activeId));

  let sidebarOpen = $state(true);
  let sidebarResizing = $state(false);
  let platform = $state<"macos" | "windows" | "linux" | "unknown">("unknown");
  let hasTitleBar = $derived(platform === "windows" || platform === "linux");
  let revealLabel = $derived(
    platform === "macos"
      ? "Show in Finder"
      : platform === "windows"
        ? "Show in File Explorer"
        : "Show in File Manager",
  );
  let favorites = $state<Location[]>([]);
  let favoritePaths = $derived(new Set(favorites.map((favorite) => favorite.path)));
  let locations = $state<Location[]>([]);
  let addLocationOpen = $state(false);
  let editingLocation = $state<Location | null>(null);
  let passwordLocation = $state<Location | null>(null);
  let passwordOpen = $state(false);
  let passwordRequest: ConnectRequest | null = null;
  let newBucketOpen = $state(false);
  let bucketSettingsOpen = $state(false);
  let bucketSettingsTarget = $state<DirectoryEntry | null>(null);
  let showHiddenFiles = $derived(settings.current.showHiddenFiles);
  let itemCheckboxes = $state(false);
  let showFps = $derived(settings.current.showFps);
  /** Pointer is over the active pane's file list, where a trackpad swipe navigates. */
  let pointerInList = $state(false);
  const jobs = new JobsStore();
  let activityOpen = $state(false);
  let finderSearch = $state<ReturnType<typeof FinderSearch>>();
  let sidebar = $state<ReturnType<typeof AppSidebar>>();
  let transferActivity = $derived(
    (() => {
      const active = jobs.jobs.find(isRunning);
      return active ? `${active.kind} ${active.label || active.destination}…` : null;
    })(),
  );

  /** Shows `optimistic` right away, then the list the backend saved; restores the old list on failure. */
  async function updateFavorites(optimistic: Location[], request: () => Promise<Location[]>) {
    const previous = favorites;
    favorites = optimistic;
    try {
      favorites = await request();
    } catch (error) {
      favorites = previous;
      await message(error instanceof Error ? error.message : String(error), {
        title: "Couldn’t update Favorites",
        kind: "error",
      });
    }
  }

  function addToFavorites(path: string, index = favorites.length) {
    const optimistic = favorites.filter((favorite) => favorite.path !== path);
    optimistic.splice(Math.min(index, optimistic.length), 0, { name: baseName(path), path, kind: "folder" });
    void updateFavorites(optimistic, () => addFavorite(path, index));
  }

  function removeFromFavorites(path: string) {
    void updateFavorites(
      favorites.filter((favorite) => favorite.path !== path),
      () => removeFavorite(path),
    );
  }

  function reorderFavoriteList(paths: string[]) {
    const byPath = new Map(favorites.map((favorite) => [favorite.path, favorite]));
    const optimistic = paths.flatMap((path) => byPath.get(path) ?? []);
    void updateFavorites(optimistic, () => reorderFavorites(paths));
  }

  function addedLocation(location: Location) {
    locations = [...locations, location];
    openAnyLocation(location);
  }

  /** Opens the local folder of a mounted network share. */
  function openMounted(location: Location, mountPath: string, options: { newTab?: boolean } = {}) {
    activeController.openLocation({ name: location.name, path: mountPath, kind: "folder" }, options);
  }

  type Credentials = { username?: string; password?: string; remember?: boolean };

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

  async function submitPassword(username: string, password: string, remember: boolean): Promise<string | null> {
    const request = passwordRequest;
    if (!request) return null;
    const outcome = await request.attempt({ username, password, remember });
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

  async function ejectVolume(location: Location) {
    try {
      await activity.action(`Eject: ${location.name}`, location.path, () =>
        invoke("eject_volume", { path: location.path }),
      );
      locations = locations.filter((candidate) => candidate.path !== location.path);
      for (const controller of controllers.values()) {
        if (controller.listingPath === location.path || controller.listingPath.startsWith(`${location.path}/`)) {
          controller.openLocation(OVERVIEW);
        }
      }
    } catch (error) {
      await message(error instanceof Error ? error.message : String(error), { title: `Couldn’t eject “${location.name}”`, kind: "error" });
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

  /** Briefly highlights new rows, as after a paste. */
  function markPasted(paths: string[]) {
    pastedPaths = new Set([...pastedPaths, ...paths]);
    setTimeout(() => (pastedPaths = new Set([...pastedPaths].filter((path) => !paths.includes(path)))), 2800);
  }

  async function handleExternalDrop(targetPaneId: string, entries: DraggedEntry[], folder?: string) {
    const target = controllerFor(targetPaneId);
    const source = entries[0] ? controllerFor(entries[0].paneId) : null;
    const destination = folder ?? target.listingPath;
    const paths = entries.map((entry) => entry.path);
    if (!source || !destination || paths.length === 0 || paths.includes(destination)) return;
    const sourceRemote = source.remoteListing || isNetworkPath(source.listingPath);
    const targetRemote = target.remoteListing || isNetworkPath(target.listingPath);
    try {
      if (!sourceRemote && !targetRemote) {
        await moveItems(paths, destination, `Move: ${paths.length} items`);
      } else if (!sourceRemote && targetRemote) {
        await uploadRemoteFiles(destination, paths);
      } else if (sourceRemote && !targetRemote) {
        await downloadRemoteItems(paths, destination);
      } else {
        for (const path of paths) await copyItem(path, destination);
      }
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
    void analytics.load();
    void invoke<string>("os_detection").then((detected) => {
      platform = detected as typeof platform;
      platformState.current = platform;
      document.documentElement.dataset.platform = detected;
    });
    void fetchFavorites().then((savedFavorites) => (favorites = savedFavorites));
    void invoke<Location[]>("locations").then((savedLocations) => {
      locations = savedLocations;
      void networkStatus.refresh().catch(() => {});
    });
    const savedWidth = Number(localStorage.getItem("sidebar-width"));
    if (savedWidth >= sidebarMinimumWidth) {
      sidebarWidth = Math.min(savedWidth, sidebarMaximumWidth);
    }
    activity.publish = (event) => jobs.upsert(event);
    extraction.onExtracted = (paths) => {
      for (const controller of controllers.values()) void controller.refreshListing(controller.listingPath);
      markPasted(paths);
    };
    const stopSettings = settings.listen();
    // Focus rings for keyboard regions show only while the keyboard is in use.
    const markKeyboard = () => (document.documentElement.dataset.keyboard = "");
    const markPointer = () => delete document.documentElement.dataset.keyboard;
    window.addEventListener("keydown", markKeyboard, true);
    // Capture F5 before focused widgets or the webview can reload the whole app.
    const refreshFolder = (event: KeyboardEvent) => {
      if (event.key !== "F5") return;
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) return;
      keyboard.cancel();
      activeController.refresh();
    };
    window.addEventListener("keydown", refreshFolder, true);
    window.addEventListener("pointerdown", markPointer, true);
    const trackSwipePointer = (event: PointerEvent) => {
      const element = document.querySelector<HTMLElement>(
        `[data-key-scope='list'][data-pane-id='${CSS.escape(panes.activeId)}']`,
      );
      if (!element) {
        pointerInList = false;
        return;
      }
      const rect = element.getBoundingClientRect();
      const inside =
        event.clientX >= rect.left &&
        event.clientX <= rect.right &&
        event.clientY >= rect.top &&
        event.clientY <= rect.bottom;
      if (inside !== pointerInList) pointerInList = inside;
    };
    window.addEventListener("pointermove", trackSwipePointer);
    // Drives mounted or ejected while the app is open (a DMG, a USB stick) reach the sidebar here.
    const volumeKey = (list: Location[]) =>
      list.filter(isVolumeLocation).map(({ path }) => path).sort().join("\n");
    const refreshVolumes = () => {
      if (document.visibilityState !== "visible") return;
      void invoke<Location[]>("refresh_locations")
        .then((fresh) => {
          if (volumeKey(fresh) !== volumeKey(locations)) locations = fresh;
        })
        .catch(() => {});
    };
    const volumePoll = setInterval(refreshVolumes, 3000);
    window.addEventListener("focus", refreshVolumes);
    const unlisteners = [
      listen<TransferEventPayload>("transfer-progress", ({ payload }) => {
        const previousProgress = fileDownloads.jobs[payload.id];
        const previousFile = previousProgress?.path;
        applyDownloadProgress(payload);
        if (payload.fileProgress?.path !== previousFile ||
            ((payload.fileProgress?.bytesDone ?? 0) > 0 && (previousProgress?.bytesDone ?? 0) === 0)) {
          const file = payload.fileProgress?.path ?? previousFile;
          if (file) {
            const parent = parentPath(file);
            for (const controller of controllers.values()) {
              const listing = controller.listingPath;
              if (parent === listing || parent === `${listing}/` || parent === `${listing}\\` || listing === payload.destination) {
                void controller.refreshListing(listing);
              }
            }
          }
        }
        jobs.upsert(payload);
        if (payload.item) transferClipboard.removeByPath(payload.item);
        extraction.applyProgress(payload);
      }),
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
      listen<boolean>("show-hidden-files", ({ payload }) => settings.set("showHiddenFiles", payload)),
      listen<boolean>("show-fps", ({ payload }) => settings.set("showFps", payload)),
      listen<boolean>("dev-tools", ({ payload }) => settings.set("prototypeSwitcher", payload)),
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
      listen<{ amount: number }>("swipe-progress", ({ payload }) => {
        if (!settings.current.swipeNavigation || prefersReducedMotion()) return;
        activeController.updateSwipe(payload.amount);
      }),
      listen<{ committed: boolean }>("swipe-end", ({ payload }) => {
        if (!settings.current.swipeNavigation) return;
        activeController.finishSwipe({ committed: payload.committed });
      }),
      listen<string>("swipe-nav", ({ payload }) => {
        if (!settings.current.swipeNavigation) return;
        if (payload === "back") activeController.goBack();
        else activeController.goForward();
      }),
      listen("open-shortcuts", () => {
        void getCurrentWindow().setFocus();
        openShortcuts();
      }),
      listen("check-for-updates", () => void updates.check({ manual: true })),
      listen("open-release-notes", () => {
        void getCurrentWindow().setFocus();
        whatsNew.show();
      }),
      // Debug menu: throw an uncaught error so it flows through the error reporter.
      listen("debug-throw-exception", () => {
        const error = new Error("Test exception thrown from the Debug menu");
        const sentry = (window as { Sentry?: { captureException: (error: unknown) => void } }).Sentry;
        console.log("[debug] throwing test exception", error, sentry ? "via window.Sentry" : "window.Sentry missing");
        sentry?.captureException(error);
        setTimeout(() => {
          throw error;
        });
      }),
    ];
    // Development builds have no published release to compare against.
    const stopUpdates = dev ? () => {} : updates.start();
    void whatsNew.checkOnLaunch();
    return () => {
      stopSettings();
      window.removeEventListener("keydown", markKeyboard, true);
      window.removeEventListener("keydown", refreshFolder, true);
      window.removeEventListener("pointerdown", markPointer, true);
      window.removeEventListener("pointermove", trackSwipePointer);
      clearInterval(volumePoll);
      window.removeEventListener("focus", refreshVolumes);
      activity.publish = () => {};
      fileDownloads.jobs = {};
      stopUpdates();
      unlisteners.forEach((unlisten) => void unlisten.then((stop) => stop()));
    };
  });

  // Native menu check items follow the settings, including changes made in the Settings window.
  $effect(() => void invoke("set_show_hidden_files", { show: settings.current.showHiddenFiles }).catch(() => {}));
  $effect(() => void invoke("set_show_fps", { show: settings.current.showFps }).catch(() => {}));
  $effect(() => {
    const visible = settings.current.prototypeSwitcher;
    devToolsVisible.set(visible);
    if (dev) void invoke("set_prototype_switcher", { visible }).catch(() => {});
  });

  // The native macOS swipe monitor navigates only when the pointer is over the active
  // list and that tab has history in the swiped direction.
  let lastSwipeContext = "";
  $effect(() => {
    if (platform !== "macos") return;
    const controller = activeController;
    const context = {
      enabled: settings.current.swipeNavigation,
      pointerInList,
      canBack: controller.canBack,
      canForward: controller.canForward,
    };
    const key = `${context.enabled}|${context.pointerInList}|${context.canBack}|${context.canForward}`;
    if (key === lastSwipeContext) return;
    lastSwipeContext = key;
    void invoke("set_swipe_context", context).catch(() => {});
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
    const entry = controller.selectedEntries.length === 1 ? controller.selectedEntries[0] : undefined;
    void invoke("set_open_target", {
      path: entry?.path ?? "",
      isDirectory: entry?.is_directory ?? false,
      enabled: Boolean(entry),
    }).catch(() => {});
  });

  function resizeSidebar(width: number) {
    sidebarWidth = Math.max(
      sidebarMinimumWidth,
      Math.min(sidebarMaximumWidth, width, window.innerWidth - sidebarMinimumWidth),
    );
    sidebarOpen = true;
    localStorage.setItem("sidebar-width", String(sidebarWidth));
  }

  // Multiple selection mode starts off each launch.
  function setItemCheckboxes(show: boolean) {
    itemCheckboxes = show;
  }

  const commands = new CommandRegistry<AppContext>();
  commands.register([...standardCommands(), ...navigationCommands()]);
  const bindings = allBindings();

  const QUICK_LOCATIONS = {
    overview: { name: "Overview", path: "", kind: "overview" },
    recents: { name: "Recents", path: "", kind: "recents" },
    shared: { name: "Shared", path: "", kind: "shared" },
  } satisfies Record<string, Location>;

  function regionElement(slot: RegionSlot): HTMLElement | null {
    if (slot.region === "sidebar") return document.querySelector<HTMLElement>("[data-key-scope='sidebar']");
    return document.querySelector<HTMLElement>(
      `[data-key-scope='${slot.region}'][data-pane-id='${CSS.escape(slot.paneId ?? "")}']`,
    );
  }

  /** Regions in F6 order that exist on screen right now. */
  function regionSlots(): RegionSlot[] {
    const slots: RegionSlot[] = sidebarOpen ? [{ region: "sidebar", paneId: null }] : [];
    for (const pane of panes.panes) {
      slots.push({ region: "list", paneId: pane.id }, { region: "preview", paneId: pane.id });
    }
    return slots.filter((slot) => regionElement(slot));
  }

  function focusedSlot(): RegionSlot {
    const element = document.activeElement?.closest<HTMLElement>("[data-key-scope]");
    const region = element?.dataset.keyScope;
    if (region === "sidebar") return { region, paneId: null };
    if ((region === "list" || region === "preview") && element?.dataset.paneId) {
      return { region, paneId: element.dataset.paneId };
    }
    return { region: "list", paneId: panes.activeId };
  }

  function focusSlot(slot: RegionSlot): boolean {
    if (slot.region === "sidebar") return sidebar?.focusItem() ?? false;
    const element = regionElement(slot);
    if (!element) return false;
    if (slot.paneId) panes.setActive(slot.paneId);
    element.focus();
    return true;
  }

  function reportCommandError(title: string, error: unknown) {
    void message(error instanceof Error ? error.message : String(error), {
      title: `Couldn’t run “${title}”`,
      kind: "error",
    });
  }

  /** Rows whose command can't run in the active pane right now are marked in the dialog. */
  function shortcutAvailable(row: ShortcutRow): boolean {
    if (!row.command) return true;
    const scope: Scope = row.scope === "global" ? "list" : row.scope;
    return commands.available(row.command, keyboardContext(scope));
  }

  const PALETTE_COMMANDS = ["app.shortcuts", "app.settings", "app.toggleKeyboardMode", "app.openTerminal", "preview.toggle", "view.list", "view.grid"];

  function shortcutFor(id: string): string {
    const keyPlatform = toKeyPlatform(platform);
    const mode = settings.current.keyboardMode;
    const matches = (binding: (typeof bindings)[number], wanted: "standard" | "yazi") =>
      binding.command === id && binding.mode === wanted && appliesTo(binding, keyPlatform);
    const binding =
      bindings.find((candidate) => matches(candidate, mode)) ??
      bindings.find((candidate) => matches(candidate, "standard")) ??
      (id === "app.settings" ? bindings.find((candidate) => candidate.menuId === "open-settings") : undefined);
    return binding ? binding.keys.map((key) => formatToken(normalizeToken(key, keyPlatform), keyPlatform)).join(" ") : "";
  }

  function paletteTitle(id: string): string {
    if (id === "app.shortcuts") return "Keyboard Shortcuts…";
    if (id === "app.settings") return "Settings…";
    if (id === "app.toggleKeyboardMode") {
      return settings.current.keyboardMode === "yazi" ? "Turn Off Yazi Mode" : "Turn On Yazi Mode";
    }
    return commands.get(id)?.title ?? id;
  }

  let paletteCommands = $derived<PaletteCommand[]>(
    [...PALETTE_COMMANDS.filter((id) => commands.available(id, keyboardContext("global"))).map((id) => ({
      id,
      title: paletteTitle(id),
      shortcut: shortcutFor(id),
      run: () => {
        const result = commands.run(id, keyboardContext("global"));
        if (result instanceof Promise) result.catch((error: unknown) => reportCommandError(paletteTitle(id), error));
      },
    })),
    ...(activeController.canCalculateSizes || activeController.sizeScanning ? [{
      id: "folder.calculateSizes",
      title: activeController.sizeScanning ? "Cancel Size Calculation" : "Calculate Folder Sizes",
      keywords: ["medir carpetas", "tamaños", "tamanos", "analizar", "directory sizes", "disk usage"],
      shortcut: "",
      run: () => activeController.sizeScanning
        ? activeController.cancelSizeScan() : void activeController.calculateSizes(),
    }] : [])],
  );

  /** What keyboard commands act on: the active pane at the moment of the key press. */
  function keyboardContext(scope: Scope): AppContext {
    const controller = activeController;
    const showHidden = showHiddenFiles;
    return {
      scope,
      list: {
        blocked: () => controller.keyboardBlocked(),
        view: () => controller.keyboardView(),
        remote: () => controller.remoteListing,
        moveFocus: (key, modifiers) => controller.moveFocus(key, modifiers, showHidden),
        moveHalfPage: (direction) => controller.moveHalfPage(direction, showHidden),
        typeSelect: (char) => controller.typeSelect(char, showHidden),
        enterFocused: () => controller.enterFocused(showHidden),
        enterOrOpenFocused: () => controller.enterOrOpenFocused(showHidden),
        openParent: () => controller.openParentFromKeyboard(),
        selectFocused: (mode) => controller.selectFocused(mode, showHidden),
        toggleFocusedAndNext: () => controller.toggleFocusedAndNext(showHidden),
        selectAll: () => controller.selectAllListed(showHidden),
        invertSelection: () => controller.invertSelection(showHidden),
        startVisual: (mode) => controller.startVisual(mode, showHidden),
        exitVisual: () => controller.exitVisual(),
        clearSelection: () => controller.clearSelectionIfAny(),
        trashSelection: (options) => controller.trashSelection(options),
        openSelection: () => controller.openSelection(),
        renameFocused: () => controller.renameFocused(showHidden),
        createItem: (kind) => controller.createFromKeyboard(kind),
        copyText: (kind) => controller.copyText(kind, showHidden),
        enqueueSelected: (mode) => controller.enqueueSelected(mode),
        canPaste: () => controller.isBrowsableFolder() && transferClipboard.items.length > 0,
        paste: () => void pasteTransferClipboard(),
        clearClipboard: () => {
          if (transferClipboard.items.length === 0) return false;
          transferClipboard.clear();
          return true;
        },
        sort: (column, dir) => controller.sortFromKeyboard(column, dir),
      },
      panes: {
        activeIndex: () => panes.panes.findIndex((pane) => pane.id === panes.activeId),
        count: () => panes.panes.length,
        activate: (index) => {
          const paneId = panes.panes[index].id;
          panes.setActive(paneId);
          focusSlot({ region: "list", paneId });
        },
        newTabInOtherPane: () => panes.newTabInOtherPane(),
        toggleSecond: () => panes.togglePane(),
      },
      tabs: {
        selectDigit: (digit) => panes.activeTabs.selectDigit(digit),
        newTab: () => panes.newTab(),
        cycle: (delta) => controller.tabs.cycle(delta),
        moveActive: (delta) => {
          const tabs = controller.tabs;
          const index = tabs.tabs.findIndex((tab) => tab.id === tabs.activeId);
          const target = index + delta;
          if (target < 0 || target >= tabs.tabs.length) return false;
          tabs.move(index, target);
          return true;
        },
        history: (direction) => {
          if (direction === "back" ? !controller.canBack : !controller.canForward) return false;
          if (direction === "back") controller.goBack();
          else controller.goForward();
          return true;
        },
      },
      checkboxes: { active: () => itemCheckboxes, exit: () => setItemCheckboxes(false) },
      preview: {
        toggle: () => (controller.previewOpen = !controller.previewOpen),
        scroll: (direction) => {
          const element = regionElement({ region: "preview", paneId: controller.paneId });
          return element ? scrollPreview(element, direction) : false;
        },
      },
      regions: {
        cycle: (direction) => {
          const target = nextRegion(regionSlots(), focusedSlot(), direction);
          return target ? focusSlot(target) : false;
        },
        focus: (region) => {
          if (region === "sidebar") {
            if (!sidebarOpen) toggleSidebar();
            void tick().then(() => sidebar?.focusItem());
            return true;
          }
          if (focusSlot({ region: "list", paneId: panes.activeId })) return true;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          return true;
        },
        sidebarMove: (delta) => sidebar?.moveFocus(delta) ?? false,
        sidebarOpen: () => sidebar?.openFocused() ?? false,
      },
      app: {
        mode: () => settings.current.keyboardMode,
        toggleKeyboardMode: () =>
          settings.set("keyboardMode", settings.current.keyboardMode === "yazi" ? "standard" : "yazi"),
        toggleHidden: () => settings.set("showHiddenFiles", !settings.current.showHiddenFiles),
        toggleActivity: () => (activityOpen = !activityOpen),
        openShortcuts: () => openShortcuts(),
        openSettings: () => invoke<void>("open_settings"),
        setView: (mode) => controller.tabs.update({ viewMode: mode }),
      },
      go: async (target) => {
        if (target === "home" || target === "downloads") {
          const path = target === "home" ? await homeDir() : await downloadDir();
          openAnyLocation({ name: baseName(path) || path, path, kind: "folder" });
          return;
        }
        openAnyLocation(QUICK_LOCATIONS[target]);
      },
      focusSearch: () => finderSearch?.focus(),
      searchContent: () => {
        controller.setSearchMode("content");
        finderSearch?.focus();
      },
      togglePalette: () => (commandPaletteState.open = !commandPaletteState.open),
      openPalette: (query) => openCommandPaletteWith(query),
      currentPath: () => controller.listingPath,
      focusInside: (selector) => Boolean(document.activeElement?.closest(selector)),
    };
  }

  const keyboard = new KeyboardDispatcher<AppContext>({
    registry: commands,
    bindings: () => bindings,
    platform: () => toKeyPlatform(platform),
    mode: () => settings.current.keyboardMode,
    timeoutMs: () => settings.current.chordTimeoutMs,
    modalOpen: () => confirmation.open || activeController.contextMenuOpen,
    context: keyboardContext,
    onError: reportCommandError,
    fallback: typeSelect,
  });

  /** Standard mode: letters no shortcut claims jump to matching names, like Finder and Explorer. */
  function typeSelect(event: KeyboardEvent, scope: Scope): boolean {
    if (scope !== "list" || settings.current.keyboardMode !== "standard") return false;
    if (event.metaKey || event.ctrlKey || event.altKey) return false;
    if ([...event.key].length !== 1 || !event.key.trim()) return false;
    const { list } = keyboardContext(scope);
    return !list.blocked() && list.typeSelect(event.key);
  }

  // Switching keyboard mode drops a half-typed chord and any visual selection.
  $effect(() => {
    void settings.current.keyboardMode;
    untrack(() => {
      keyboard.cancel();
      for (const controller of controllers.values()) controller.exitVisual();
    });
  });

  function cancelTransferClipboard() {
    pasteCancelled = true;
    if (pasteJobId) jobs.cancel(pasteJobId);
    transferClipboard.clear();
    transferClipboardOpen = false;
  }

  async function pasteTransferClipboard() {
    if (pasteInProgress) return;
    const request = transferClipboard.beginPaste(activeController.listingPath);
    if (!request) return;
    pasteInProgress = true;
    pasteCancelled = false;
    const copy = request.entries.filter((entry) => entry.mode === "copy");
    const move = request.entries.filter((entry) => entry.mode === "move");
    for (const entry of request.entries) {
      transferClipboard.applyProgress({
        jobId: "clipboard",
        itemId: entry.id,
        state: "transferring",
      });
    }
    try {
      // One job (one progress bar) for the whole paste. The backend emits an
      // event per finished top-level item that drains the list above.
      const created: string[] = [];
      if (copy.length) {
        const entries = await copyItems(
          copy.map((entry) => entry.path),
          request.destination,
          `Copy: ${copy.length} items`,
          (id) => (pasteJobId = id),
        );
        pasteJobId = null;
        created.push(...entries.map((entry) => entry.path));
      }
      if (move.length && !pasteCancelled) {
        const entries = await moveItems(
          move.map((entry) => entry.path),
          request.destination,
          `Move: ${move.length} items`,
          (id) => (pasteJobId = id),
        );
        pasteJobId = null;
        created.push(...entries.map((entry) => entry.path));
      }
      if (!pasteCancelled) for (const entry of request.entries) transferClipboard.removeByPath(entry.path);
      markPasted(created);
    } catch (error) {
      // Keep the queued entries available for retry when the operation fails.
      if (!pasteCancelled) for (const entry of request.entries) {
        transferClipboard.applyProgress({
          jobId: "clipboard",
          itemId: entry.id,
          state: "failed",
          error: error instanceof Error ? error.message : String(error),
        });
      }
    } finally {
      pasteJobId = null;
      pasteInProgress = false;
    }
    activeController.reload();
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
<svelte:window onkeydown={keyboard.handle} />

{#if platform === "windows" || platform === "linux"}
  <TitleBar {platform} />
{/if}
<Sidebar.Provider
  bind:open={sidebarOpen}
  onToggle={toggleSidebar}
  style={`--sidebar-width: ${sidebarWidth}px;`}
  class={`finder-window platform-${platform}${sidebarResizing ? " sidebar-resizing" : ""}${sidebarOpen ? "" : " sidebar-collapsed"}`}>
  <AppSidebar
    bind:this={sidebar}
    selected={activeController.tabs.active.location}
    {favorites}
    {locations}
    onOpen={(location) => openAnyLocation(location)}
    onAddLocation={() => (addLocationOpen = true)}
    onRemoveLocation={removeLocation}
    onEditLocation={editLocation}
    onCopyLocationAddress={copyLocationAddress}
    onDisconnectLocation={(location) => void disconnectLocation(location)}
    onEjectLocation={(location) => void ejectVolume(location)}
    onAddFavorite={addToFavorites}
    onRemoveFavorite={(favorite) => removeFromFavorites(favorite.path)}
    onReorderFavorites={reorderFavoriteList}
    statuses={networkStatus.statuses}
    open={sidebarOpen}
    width={sidebarWidth}
    onResize={resizeSidebar}
    onResizeStart={() => (sidebarResizing = true)}
    onResizeEnd={() => (sidebarResizing = false)}
    onToggle={toggleSidebar}
    variant="sidebar"
    onOpenPalette={openCommandPalette}
    brand={hasTitleBar} />
  {#if !hasTitleBar}
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
  {#if analytics.loaded && !analytics.welcomeSeen}
    <WelcomeDialog />
  {/if}
  <ConflictHost />
  <UpdateBanner />
  <ChangelogDialog bind:open={whatsNew.open} version={whatsNew.version} notes={whatsNew.notes} />
  <CommandPalette commands={paletteCommands} {favorites} {locations} recents={activeController.recents} onNavigate={(location, opts) => openAnyLocation(location, opts)} />
  <WhichKey pending={keyboard.pending} enabled={settings.current.showWhichKey} platform={toKeyPlatform(platform)} />
  <ShortcutsDialog
    {bindings}
    platform={toKeyPlatform(platform)}
    mode={settings.current.keyboardMode}
    available={shortcutAvailable}
    onToggleMode={() => void commands.run("app.toggleKeyboardMode", keyboardContext("global"))} />
  <Sidebar.Inset class="finder-content">
    <header class="finder-toolbar" data-tauri-drag-region="deep">
      <div class="toolbar-leading">
        {#if hasTitleBar && !sidebarOpen}
          <img class="toolbar-brand" src="/app-icon.png" alt="Lite Explorer" width="18" height="18" />
          <button
            class="sidebar-trigger sidebar-trigger-inline"
            aria-label="Toggle sidebar"
            title="Toggle sidebar"
            onclick={toggleSidebar}><PanelLeftIcon /></button>
        {/if}
        <div class="toolbar-controls">
          <button aria-label="Back" disabled={!activeController.canBack} onclick={() => activeController.goBack()}><ArrowLeftIcon /></button>
          <button aria-label="Forward" disabled={!activeController.canForward} onclick={() => activeController.goForward()}><ArrowRightIcon /></button>
        </div>
      </div>
      <button class="location-title" aria-label="Go to folder" onclick={() => openCommandPaletteWith(withTrailingSlash(activeController.listingPath))}
        >{activeController.selected}<ChevronDownIcon /></button>
      <div class="toolbar-actions">
        {#if activeController.tabs.active.location.kind !== "overview"}
          <div class="toolbar-group">
            <button aria-label="List view" aria-pressed={activeController.viewMode === "list"} onclick={() => activeController.tabs.update({ viewMode: "list" })}><ListIcon /></button>
            <button aria-label="Icon view" aria-pressed={activeController.viewMode === "grid"} onclick={() => activeController.tabs.update({ viewMode: "grid" })}><Grid2X2Icon /></button>
          </div>
          <div class="toolbar-group">
            <button
              aria-label={itemCheckboxes ? "Hide item checkboxes" : "Show item checkboxes"}
              aria-pressed={itemCheckboxes}
              title="Item Checkboxes"
              onclick={() => setItemCheckboxes(!itemCheckboxes)}><ListChecksIcon /></button>
            <button
              aria-label={activeController.previewOpen ? "Hide preview" : "Show preview"}
              aria-pressed={activeController.previewOpen}
              title={activeController.previewOpen ? "Hide preview" : "Show preview"}
              onclick={() => (activeController.previewOpen = !activeController.previewOpen)}>{#if activeController.previewOpen}<PanelRightCloseIcon />{:else}<PanelRightOpenIcon />{/if}</button>
            <button
              aria-label={activeController.sizeScanning ? "Cancel Size Calculation" : "Calculate Folder Sizes"}
              title={activeController.sizeScanning ? "Cancel Size Calculation" : `Calculate Folder Sizes · ${activeController.selected}`}
              disabled={!activeController.canCalculateSizes && !activeController.sizeScanning}
              onclick={() => activeController.sizeScanning ? activeController.cancelSizeScan() : void activeController.calculateSizes()}>
              {#if activeController.sizeScanning}
                <LoaderCircleIcon class="animate-spin text-[#5cb9ff] motion-reduce:animate-none" />
              {:else}
                <CalculatorIcon />
              {/if}
            </button>
          </div>
        {/if}
        <div class="toolbar-group">
          <button
            aria-label="Toggle second pane"
            aria-pressed={panes.panes.length > 1}
            title="Show Second Pane"
            onclick={() => panes.togglePane()}><Columns2Icon /></button>
          <button
            aria-label="Toggle pane orientation"
            title="Split Horizontally/Vertically"
            onclick={() => panes.toggleLayout()}><Rows2Icon /></button>
        </div>
        <div class="toolbar-group">
          <button
            aria-label="Activity"
            aria-pressed={activityOpen}
            title="Activity"
            class="activity-toggle"
            onclick={() => (activityOpen = !activityOpen)}>
            <RotateCwIcon />
            {#if jobs.activeCount > 0}<span class="activity-badge">{jobs.activeCount}</span>{/if}
          </button>
        </div>
        <FinderSearch bind:this={finderSearch} controller={activeController} />
      </div>
    </header>

    <div class="panes" data-layout={panes.layout}>
      {#each panes.panes as pane (pane.id)}
        <FilePane
          controller={controllerFor(pane.id)}
          active={pane.id === panes.activeId}
          onActivate={() => panes.setActive(pane.id)}
          {showHiddenFiles}
          {itemCheckboxes}
          onEnableCheckboxes={() => setItemCheckboxes(true)}
          {showFps}
          {revealLabel}
          {transferActivity}
          onOpenBucketSettings={openBucketSettings}
          onNewBucket={() => (newBucketOpen = true)}
          onReconnect={(location) => reconnect(controllerFor(pane.id), location)}
          onExternalDrop={(paths, options) => handleExternalDrop(pane.id, paths, options)}
          onCrossPaneDrop={(from, to, fromIndex, toIndex) => panes.moveTab(from, to, fromIndex, toIndex)}
          {favoritePaths}
          {pastedPaths}
          keyboardMode={settings.current.keyboardMode}
          pendingKeys={pane.id === panes.activeId && keyboard.pending
            ? keyboard.pending.typed.map((token) => formatToken(token, toKeyPlatform(platform))).join(" ")
            : ""}
          onToggleFavorite={(entry, add) => (add ? addToFavorites(entry.path) : removeFromFavorites(entry.path))} />
      {/each}
    </div>
    <ActivityDrawer {jobs} open={activityOpen} onToggle={() => (activityOpen = !activityOpen)} />
    <TransferToast {jobs} />
    {#if transferClipboard.items.length}
      <aside class:open={transferClipboardOpen} class="transfer-clipboard" aria-label="Transfer clipboard">
        <button class="transfer-clipboard-stack" onclick={() => (transferClipboardOpen = !transferClipboardOpen)} aria-expanded={transferClipboardOpen}>
          <span class="transfer-clipboard-layer"></span><span class="transfer-clipboard-layer"></span>
          <span class="transfer-clipboard-front"><CopyIcon class="size-4" /> {transferClipboard.items.length} ready</span>
        </button>
        {#if transferClipboardOpen}
          <div class="transfer-clipboard-panel">
            <div class="transfer-clipboard-heading"><strong>Ready to paste</strong><button aria-label="Cancel transfer clipboard" onclick={cancelTransferClipboard}><XIcon class="size-4" /></button></div>
            {#each transferClipboard.items as item (item.id)}
              <div class="transfer-clipboard-item"><span class="min-w-0 truncate">{item.name}</span><span>{item.mode === "copy" ? "Copy" : "Move"}</span><button aria-label={`Remove ${item.name}`} onclick={() => transferClipboard.remove(item.id)}><XIcon class="size-3" /></button></div>
            {/each}
            <div class="transfer-clipboard-actions">
              <button class="transfer-clipboard-cancel" onclick={cancelTransferClipboard}>Cancel</button>
              {#if activeController.isBrowsableFolder()}
                <button class="transfer-clipboard-paste" disabled={pasteInProgress} onclick={() => void pasteTransferClipboard()}>Paste here</button>
              {/if}
            </div>
          </div>
        {/if}
      </aside>
    {/if}
  </Sidebar.Inset>
</Sidebar.Provider>
<DragGhost />
