import { activity } from "$lib/transfers/jobs.js";
import { Channel, invoke } from "@tauri-apps/api/core";
import { message, open } from "@tauri-apps/plugin-dialog";
import {
  compressExtension,
  createArchive,
  detectSevenZip,
  type CompressFormat,
} from "$lib/file-ops/archive.js";
import { extraction } from "$lib/archive/extraction.svelte.js";
import { confirmation } from "$lib/components/custom/dialog/index.js";
import type { DirectoryEntry } from "$lib/components/custom/file-list/index.js";
import {
  chooseApplication,
  fetchDefaultApp,
  fetchOpenWithApps,
  openTarget,
  openWithTarget,
  revealPath,
  type OpenWithApp,
} from "$lib/file-ops/open.js";
import {
  baseName,
  copyItem,
  copyItems,
  deleteItems,
  moveItem,
  moveItems,
  parentPath,
  trashItems,
} from "$lib/file-ops/files.js";
import { createItemActions, nextDefaultName, type CreateKind } from "$lib/file-ops/items.js";
import { isNetworkPath, isServerPath } from "$lib/remote/network-locations.js";
import { networkStatus } from "$lib/remote/network-status.svelte.js";
import {
  deleteRemoteBucket,
  deleteRemoteItems,
  downloadRemoteFile,
  downloadRemoteItems,
  isRemotePath,
  toS3Uri,
  uploadRemoteFiles,
  type CreatedBucket,
} from "$lib/remote/remote-locations.js";
import type { Location } from "$lib/tabs/tabs.js";
import type { TabsStore } from "$lib/tabs/tabs.svelte.js";
import type { ClipboardMode, QueueEntry } from "$lib/transfer-clipboard/queue.js";
import {
  EMPTY_SELECTION,
  addRange,
  applyNav,
  focusAfterRemoval,
  halfPageTarget,
  invert,
  navTarget,
  prune,
  selectAll,
  selectOnly,
  selectRange,
  toggle,
  visualRange,
  type NavKey,
  type NavLayout,
  type Selection,
  type VisualState,
} from "$lib/selection/selection.js";
import { settings } from "$lib/settings/settings.svelte.js";
import type { SortColumn, SortDir } from "$lib/components/custom/file-list/sort.js";
import { mergeListing, removePaths, renameEntry, upsertEntry } from "./listing-merge.js";
import type { CopyTextKind } from "$lib/keyboard/context.js";
import { fileStem } from "$lib/keyboard/text.js";
import type { SwipeDirection } from "$lib/swipe/gesture.js";

type Recent = { name: string; path: string; kind: string; opened_at: number };
export type SearchMode = "fuzzy" | "content";
export type SearchEntry = DirectoryEntry & {
  relative_path: string;
  snippet?: string;
  inner_path?: string;
};
type SearchResponse = { results: SearchEntry[]; skipped: number; limited: boolean };

/** What the mounted file list exposes so keyboard selection follows its visible order and layout. */
export type ListNavigator = {
  /** Visible entries in display order (sorted, hidden files filtered). */
  order(): DirectoryEntry[];
  view(): "list" | "grid";
  columns(): number;
  pageRows(): number;
  scrollToIndex(index: number): void;
  /** Sorts the list by a column and remembers it for the folder. */
  sort(column: SortColumn, dir: SortDir): void;
};

/** A click previews after this delay, so a double-click can open instead without flashing the preview. */
const PREVIEW_CLICK_DELAY = 250;

/** S3 objects and SFTP/FTP files: changed through backend commands and opened from a cached download. */
function isRemoteLike(path: string): boolean {
  return isRemotePath(path) || isServerPath(path);
}
function targetLabel(targets: DirectoryEntry[]): string {
  return targets.length === 1 ? targets[0].name : `${targets.length} items`;
}

export class FilePaneController {
  entries = $state<DirectoryEntry[]>([]);
  searchQuery = $state("");
  searchMode = $state<SearchMode>("fuzzy");
  searchResults = $state<SearchEntry[] | null>(null);
  searchPending = $state(false);
  searchSkipped = $state(0);
  searchLimited = $state(false);
  #searchRequest = 0;
  #searchTimer: ReturnType<typeof setTimeout> | undefined;
  recents = $state<Recent[]>([]);
  listing = $state(false);
  listingError = $state("");
  sizeScanning = $state(false);
  sizeScanMessage = $state("");
  #sizeRequest = "";

  get canCalculateSizes() {
    return (
      this.isBrowsableFolder() &&
      !this.remoteListing &&
      !isNetworkPath(this.listingPath) &&
      !this.listing &&
      !this.listingError &&
      !this.disconnected &&
      !this.searchQuery.trim()
    );
  }

  async calculateSizes() {
    if (!this.canCalculateSizes || this.sizeScanning) return;
    const path = this.listingPath;
    const requestId = crypto.randomUUID();
    this.#sizeRequest = requestId;
    this.sizeScanning = true;
    this.sizeScanMessage = "";
    for (const entry of this.entries) {
      if (entry.is_directory) {
        entry.size = undefined;
        entry.sizeComplete = false;
      }
    }
    this.navigator?.sort("size", "desc");
    const channel = new Channel<{
      sizes: { path: string; size: number; complete: boolean }[];
      done: boolean;
      error: string | null;
    }>();
    channel.onmessage = (update) => {
      if (this.#sizeRequest !== requestId || this.listingPath !== path) return;
      for (const item of update.sizes) {
        const index = this.entryIndex.get(item.path);
        if (index !== undefined) {
          this.entries[index].size = item.size;
          this.entries[index].sizeComplete = item.complete;
        }
      }
      if (update.done) {
        this.#sizeRequest = "";
        this.sizeScanning = false;
        this.sizeScanMessage = update.error
          ? `Couldn’t calculate sizes: ${update.error}`
          : this.entries.some((entry) => entry.sizeComplete === false)
            ? "Sizes calculated · some items could not be read"
            : "Sizes calculated";
      }
    };
    try {
      await invoke("scan_directory_sizes", { path, requestId, onProgress: channel });
      // Cancellation can arrive while the start command is still being dispatched.
      if (this.#sizeRequest !== requestId)
        void invoke("cancel_directory_size_scan", { requestId }).catch(console.error);
    } catch (error) {
      if (this.#sizeRequest !== requestId) return;
      this.#sizeRequest = "";
      this.sizeScanning = false;
      this.sizeScanMessage = `Couldn’t calculate sizes: ${String(error)}`;
    }
  }

  cancelSizeScan() {
    const requestId = this.#sizeRequest;
    this.#sizeRequest = "";
    if (!requestId) return;
    this.sizeScanning = false;
    this.sizeScanMessage = "Calculation cancelled · partial sizes kept";
    void invoke("cancel_directory_size_scan", { requestId }).catch(console.error);
  }
  /** Trackpad swipe in progress: the listing follows the finger. Amount is -1..1. */
  swipeActive = $state(false);
  swipeSettling = $state(false);
  swipeAmount = $state(0);
  swipeDirection = $state<SwipeDirection | null>(null);
  /** Bumped when the adjacent-folder cache changes, so `swipeEntries` stays reactive. */
  prefetchVersion = $state(0);
  /** Saved network location whose share or session is gone, shown with a Reconnect button. */
  disconnected = $state<Location | null>(null);
  renamingPath = $state("");
  /** Entries the open context menu acts on: the selection when the menu opened on it. */
  contextTargets = $state<DirectoryEntry[]>([]);
  /** Targets captured while the compress dialog is open. */
  compressTargets = $state<DirectoryEntry[]>([]);
  compressDialogOpen = $state(false);
  /** Detected 7-Zip binary path, `null` when it is not installed. */
  sevenZipPath = $state<string | null>(null);
  #sevenZipLoaded = false;
  previewEntryPath = $state("");
  previewOpen = $state(settings.current.previewOpenByDefault);
  /** Yazi visual mode: keyboard moves select a range from its anchor. */
  visual = $state<VisualState | null>(null);
  contextMenuOpen = $state(false);
  remoteDropActive = $state(false);
  openWithApps = $state<OpenWithApp[]>([]);
  defaultApp = $state<OpenWithApp | null>(null);
  /** Set by the app shell so both panes feed the same transfer clipboard. */
  onTransferClipboard: (entry: QueueEntry) => void = () => {};

  scrollPositions = new Map<string, number>();
  /** Registered by the mounted file list. */
  navigator: ListNavigator | null = null;
  #previewTimer: ReturnType<typeof setTimeout> | undefined;
  /** Directory listings of adjacent history locations, keyed by path. */
  #listingCache = new Map<string, DirectoryEntry[]>();
  #prefetchTimer: ReturnType<typeof setTimeout> | undefined;
  #swipeToken = 0;

  recentEntries = $derived(
    this.recents.map((recent) => ({
      name: recent.name,
      path: recent.path,
      is_directory: recent.kind !== "file",
      is_hidden: false,
      opened_at: recent.opened_at,
    })),
  );
  entryIndex = $derived(
    new Map(this.entries.map((entry, i) => [entry.path, i] as [string, number])),
  );

  get viewMode() {
    return this.tabs.active.viewMode;
  }
  get canBack() {
    return this.tabs.active.back.length > 0;
  }
  get canForward() {
    return this.tabs.active.forward.length > 0;
  }
  get listingPath() {
    return this.tabs.active.location.path;
  }
  get remoteListing() {
    return isRemoteLike(this.listingPath);
  }
  get remoteRoot() {
    return this.remoteListing && toS3Uri(this.listingPath) === "s3://";
  }
  /** The share list of an SMB server: entries open or mount shares, nothing else applies. */
  get serverRoot() {
    return isNetworkPath(this.listingPath) && !isServerPath(this.listingPath);
  }
  /** Opens a share that isn't mounted yet (`smb://<id>/<share>`). Set by the page. */
  openShare: (path: string, options: { newTab?: boolean }) => void = () => {};
  get selection(): Selection {
    return this.tabs.active.selection;
  }
  get focusPath() {
    return this.tabs.active.selection.focus;
  }
  /** Single entry the context menu acts on; null when it opened on several. */
  get contextTarget(): DirectoryEntry | null {
    return this.contextTargets.length === 1 ? this.contextTargets[0] : null;
  }
  selectedSet = $derived(new Set(this.selection.paths));
  /** Selected entries in listing order. */
  selectedEntries = $derived.by(() => {
    const selected = this.selectedSet;
    if (selected.size === 0) return [];
    return this.sourceEntries.filter((entry) => selected.has(entry.path));
  });
  get sourceEntries(): DirectoryEntry[] {
    return this.selected === "Recents" ? this.recentEntries : this.visibleEntries;
  }
  get selected() {
    return this.tabs.active.location.name;
  }
  get visibleEntries(): DirectoryEntry[] {
    return this.searchResults ?? this.entries;
  }

  setSearchQuery(query: string) {
    this.searchQuery = query;
    this.runSearch();
  }
  setSearchMode(mode: SearchMode) {
    this.searchMode = mode;
    this.runSearch();
  }
  clearSearch() {
    clearTimeout(this.#searchTimer);
    this.#searchRequest++;
    this.searchQuery = "";
    this.searchResults = null;
    this.searchPending = false;
    this.searchSkipped = 0;
    this.searchLimited = false;
  }
  private runSearch() {
    clearTimeout(this.#searchTimer);
    const query = this.searchQuery.trim();
    const path = this.listingPath;
    const request = ++this.#searchRequest;
    if (!query || !path || this.remoteListing || this.serverRoot) {
      this.searchResults = null;
      this.searchPending = false;
      return;
    }
    this.searchPending = true;
    this.#searchTimer = setTimeout(async () => {
      try {
        const response = await invoke<SearchResponse>("search_directory", {
          path,
          query,
          mode: this.searchMode,
        });
        if (
          request === this.#searchRequest &&
          path === this.listingPath &&
          query === this.searchQuery.trim()
        ) {
          this.searchResults = response.results;
          this.searchSkipped = response.skipped;
          this.searchLimited = response.limited;
        }
      } catch (error) {
        if (request === this.#searchRequest) {
          this.searchResults = null;
          this.searchSkipped = 0;
          this.searchLimited = false;
        }
        console.error("search_directory failed", error);
      } finally {
        if (request === this.#searchRequest) this.searchPending = false;
      }
    }, 120);
  }

  constructor(
    readonly paneId: string,
    readonly tabs: TabsStore,
    private readonly onWindowClose: () => void,
  ) {}

  /** Called by the mounted file list; returns its unregister function. */
  registerNavigator(navigator: ListNavigator): () => void {
    this.navigator = navigator;
    return () => {
      if (this.navigator === navigator) this.navigator = null;
    };
  }

  /** Stores the selection on the tab. One selected entry is previewed; several show a summary. */
  setSelection(selection: Selection, { delayPreview = false }: { delayPreview?: boolean } = {}) {
    this.tabs.update({ selection });
    clearTimeout(this.#previewTimer);
    const path = selection.paths.length === 1 ? selection.paths[0] : "";
    if (delayPreview && path && path !== this.previewEntryPath) {
      this.#previewTimer = setTimeout(() => (this.previewEntryPath = path), PREVIEW_CLICK_DELAY);
    } else {
      this.previewEntryPath = path;
    }
  }

  selectEntry(entry: DirectoryEntry) {
    this.setSelection(selectOnly(entry.path));
  }

  clearSelection() {
    this.setSelection(EMPTY_SELECTION);
  }

  /** Paths of the listed entries in display order. */
  private orderPaths(): string[] {
    return (this.navigator?.order() ?? this.sourceEntries).map((entry) => entry.path);
  }

  /** Click with Explorer modifiers: primary toggles, Shift selects a range, both add a range. */
  clickEntry(entry: DirectoryEntry, { primary, shift }: { primary: boolean; shift: boolean }) {
    const current = this.selection;
    let next: Selection;
    if (shift && primary) next = addRange(current, this.orderPaths(), entry.path);
    else if (shift) next = selectRange(current, this.orderPaths(), entry.path);
    else if (primary) next = toggle(current, entry.path);
    else next = selectOnly(entry.path);
    this.setSelection(next, { delayPreview: !shift && !primary });
  }

  toggleEntry(entry: DirectoryEntry) {
    this.setSelection(toggle(this.selection, entry.path));
  }

  /** Header checkbox: selects everything unless everything is already selected. */
  toggleAll() {
    const order = this.orderPaths();
    const all = order.length > 0 && order.every((path) => this.selectedSet.has(path));
    this.setSelection(all ? EMPTY_SELECTION : selectAll(order));
  }

  /** Keeps only selected paths that are still listed. */
  pruneSelection() {
    const next = prune(
      this.selection,
      this.sourceEntries.map((entry) => entry.path),
    );
    if (next !== this.selection) this.setSelection(next);
  }

  enqueueSelected(mode: ClipboardMode): boolean {
    if (this.serverRoot) return false;
    const entries = this.selectedEntries;
    if (entries.length === 0) return false;
    this.enqueue(entries, mode);
    return true;
  }

  enqueueContextTargets(mode: ClipboardMode) {
    if (this.serverRoot) return;
    this.enqueue(this.contextTargets, mode);
  }

  private enqueue(entries: DirectoryEntry[], mode: ClipboardMode) {
    for (const entry of entries) {
      this.onTransferClipboard({
        path: entry.path,
        name: entry.name,
        isDirectory: entry.is_directory,
        mode,
      });
    }
  }

  openLocation(location: Location, options: { newTab?: boolean } = {}) {
    if (options.newTab) {
      this.tabs.open(location);
    } else {
      this.scrollPositions.delete(this.tabs.activeId);
      this.tabs.navigate(location);
    }
    if (
      location.path &&
      !isRemoteLike(location.path) &&
      !isNetworkPath(location.path) &&
      !networkStatus.mountFor(location.path)
    ) {
      void invoke("record_recent", {
        path: location.path,
        name: location.name,
        kind: location.kind,
      }).catch(() => {});
    }
  }

  async loadLocation(location: Location) {
    this.cancelSizeScan();
    this.sizeScanMessage = "";
    this.#endSwipe();
    this.clearSearch();
    this.visual = null;
    const token = ++this.loadToken;
    this.renamingPath = "";
    this.listingError = "";
    this.disconnected = null;
    if (location.kind === "recents") {
      this.listing = false;
      this.entries = [];
      const result = await invoke<Recent[]>("recents");
      if (token === this.loadToken) {
        this.recents = result;
        this.pruneSelection();
      }
      return;
    }
    if (!location.path) {
      this.listing = false;
      this.entries = [];
      return;
    }
    // Going back or forward to a prefetched folder shows it right away, then refreshes.
    const cached = this.#listingCache.get(location.path);
    if (cached) {
      this.entries = cached;
      this.listing = false;
    } else {
      this.listing = true;
    }
    try {
      const result = await invoke<DirectoryEntry[]>("read_directory", { path: location.path });
      if (token !== this.loadToken) return;
      this.entries = result;
      this.#listingCache.set(location.path, result);
      this.prefetchAdjacent();
      this.pruneSelection();
      const owner = isNetworkPath(location.path) ? networkStatus.ownerOf(location.path) : null;
      if (owner && !isServerPath(location.path)) {
        const mounted = result.filter((entry) => !isNetworkPath(entry.path));
        networkStatus.rememberShares(
          owner,
          mounted.map((entry) => ({ name: entry.name, path: entry.path })),
        );
      }
    } catch (error) {
      if (token !== this.loadToken) return;
      this.entries = [];
      const owner = networkStatus.ownerOf(location.path);
      const gone = owner ? !(await networkStatus.isConnected(owner).catch(() => true)) : false;
      if (token !== this.loadToken) return;
      if (gone) this.disconnected = owner;
      else this.listingError = error instanceof Error ? error.message : String(error);
    } finally {
      if (token === this.loadToken) this.listing = false;
    }
    // Size scans over a network share would read the whole server.
    if (
      token === this.loadToken &&
      !isRemoteLike(location.path) &&
      !isNetworkPath(location.path) &&
      !networkStatus.mountFor(location.path)
    ) {
      void invoke("compute_directory_sizes", { path: location.path }).catch(() => {});
    }
  }

  private loadToken = 0;

  /** Lists the active tab's folder again, for example after reconnecting its share. */
  reload() {
    void this.loadLocation(this.tabs.active.location);
  }

  /** Reads the adjacent history folders ahead of a swipe, so the incoming list can slide in.
   *  Remote and network locations are left to the regular loading state. */
  prefetchAdjacent() {
    clearTimeout(this.#prefetchTimer);
    this.#prefetchTimer = setTimeout(() => {
      const tab = this.tabs.active;
      for (const location of [tab.back.at(-1), tab.forward[0]]) {
        if (this.#canPrefetch(location)) void this.#prefetch(location);
      }
    }, 250);
  }

  #canPrefetch(location: Location | undefined): location is Location {
    return (
      !!location &&
      !!location.path &&
      location.kind === "folder" &&
      !isRemoteLike(location.path) &&
      !isNetworkPath(location.path)
    );
  }

  async #prefetch(location: Location) {
    if (this.#listingCache.has(location.path)) return;
    try {
      const entries = await invoke<DirectoryEntry[]>("read_directory", { path: location.path });
      this.#listingCache.set(location.path, entries);
      this.prefetchVersion++;
    } catch {
      // Prefetch is best effort; a real navigation reports the error.
    }
  }

  /** First adjacent location on a side and its prefetched entries, or null without history. */
  swipeTarget(
    direction: SwipeDirection,
  ): { path: string; entries: DirectoryEntry[] | null } | null {
    void this.prefetchVersion;
    const tab = this.tabs.active;
    const location = direction === "back" ? tab.back.at(-1) : tab.forward[0];
    if (!location) return null;
    return { path: location.path, entries: this.#listingCache.get(location.path) ?? null };
  }

  /** Address shown and copied for a path: the server address for network locations. */
  displayPath(path: string): string {
    return networkStatus.address(path) ?? toS3Uri(path);
  }

  async clearRecents() {
    this.recents = [];
    await invoke("clear_recents").catch(() => {});
  }

  /** Returns true when the window should close. */
  closeTab(id: string): boolean {
    if (this.tabs.close(id)) {
      this.onWindowClose();
      return true;
    }
    this.scrollPositions.delete(id);
    return false;
  }

  openEntry(entry: DirectoryEntry, options: { newTab?: boolean } = {}) {
    clearTimeout(this.#previewTimer);
    if (entry.kind === "share" && isNetworkPath(entry.path)) {
      this.openShare(entry.path, options);
      return;
    }
    if (entry.is_directory) {
      this.openLocation({ name: entry.name, path: entry.path, kind: "folder" }, options);
      return;
    }
    if (isRemoteLike(entry.path)) {
      void this.openPath(entry.path);
      return;
    }
    void openTarget(entry.path).catch((error) => {
      this.listingError = error instanceof Error ? error.message : String(error);
    });
    void invoke("record_recent", { path: entry.path, name: entry.name, kind: "file" }).catch(
      () => {},
    );
  }

  /** Opens a file with its default app. Remote objects are downloaded to the app cache first;
   *  edits to that copy are not uploaded. */
  async openPath(path: string) {
    if (!isRemoteLike(path)) {
      await openTarget(path);
      return;
    }
    if (path.endsWith("/")) {
      const name = toS3Uri(path).split("/").filter(Boolean).pop() ?? path;
      this.openLocation({ name, path, kind: "folder" });
      return;
    }
    try {
      await openTarget(await downloadRemoteFile(path));
    } catch (error) {
      await message(error instanceof Error ? error.message : String(error), {
        title: "Couldn’t open file",
        kind: "error",
      });
    }
  }

  applyDirectorySizes(payload: { path: string; sizes: { path: string; size: number }[] }) {
    if (payload.path !== this.listingPath) return;
    for (const item of payload.sizes) {
      const index = this.entryIndex.get(item.path);
      if (index !== undefined && this.entries[index].sizeComplete == null)
        this.entries[index].size = item.size;
    }
  }

  isBrowsableFolder(): boolean {
    return (
      Boolean(this.listingPath) &&
      this.selected !== "Overview" &&
      this.selected !== "Recents" &&
      !this.remoteRoot &&
      !this.serverRoot
    );
  }

  async createItem(kind: CreateKind) {
    if (!this.isBrowsableFolder()) return;
    const name = nextDefaultName(kind);
    try {
      const entry = await createItemActions[kind].create(this.listingPath, name);
      this.addEntry(entry);
      this.selectEntry(entry);
      this.renamingPath = entry.path;
    } catch (error) {
      if (this.remoteListing) return this.showError(`Couldn’t create ${name}`, error);
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  async renameItem(oldPath: string, newName: string) {
    try {
      await activity.track(
        "rename",
        `Rename: ${baseName(oldPath)} → ${newName}`,
        parentPath(oldPath),
        () => invoke("rename_item", { path: oldPath, newName }),
      );
      this.applyRename(oldPath, newName);
    } catch (error) {
      if (isRemoteLike(oldPath)) void this.showError("Couldn’t rename", error);
      else this.listingError = error instanceof Error ? error.message : String(error);
    } finally {
      this.renamingPath = "";
    }
  }

  /** Renames the listed entry without re-reading the folder, so its measured size stays. */
  private applyRename(oldPath: string, newName: string) {
    const index = this.entryIndex.get(oldPath);
    if (index === undefined) return;
    const newPath = `${parentPath(oldPath)}${newName}`;
    this.setEntries(renameEntry(this.entries, oldPath, newPath, newName));
    const { paths, anchor, focus } = this.selection;
    if (!paths.includes(oldPath) && anchor !== oldPath && focus !== oldPath) return;
    const swap = (path: string) => (path === oldPath ? newPath : path);
    this.setSelection({
      paths: paths.map(swap),
      anchor: swap(anchor),
      focus: swap(focus),
    });
  }

  /** Adds a created or pasted entry to the listing without re-reading the folder. */
  addEntry(entry: DirectoryEntry) {
    this.setEntries(upsertEntry(this.entries, entry));
  }

  /** Drops deleted entries from the listing right away, keeping the sizes already measured. */
  removeEntries(paths: string[]) {
    if (paths.length === 0) return;
    this.setEntries(removePaths(this.entries, paths));
    this.pruneSelection();
  }

  /** Replaces the listing and keeps the copy used by history navigation in sync. */
  private setEntries(entries: DirectoryEntry[]) {
    this.entries = entries;
    if (this.listingPath) this.#listingCache.set(this.listingPath, entries);
  }

  async showError(title: string, error: unknown) {
    await message(error instanceof Error ? error.message : String(error), { title, kind: "error" });
  }

  /** Folds a fresh read of the folder into the listing: measured sizes and the scan survive. */
  async refreshListing(path: string) {
    if (this.listingPath !== path || !path || this.selected === "Recents") return;
    try {
      const result = await invoke<DirectoryEntry[]>("read_directory", { path });
      if (this.listingPath !== path) return;
      this.setEntries(mergeListing(this.entries, result));
      this.pruneSelection();
    } catch (error) {
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  renameContextTarget() {
    if (this.contextTarget) this.renamingPath = this.contextTarget.path;
  }

  deleteContextTargets() {
    if (this.contextTargets.length) this.deleteEntries(this.contextTargets, { permanent: true });
  }

  deleteEntries(targets: DirectoryEntry[], { permanent }: { permanent: boolean }) {
    if (targets.length === 0) return;
    const folder = this.listingPath;
    const bucket = this.remoteRoot;
    const remote = targets.some((target) => isRemoteLike(target.path));
    const irreversible = permanent || remote;
    const single = targets.length === 1 ? targets[0] : null;
    const order = this.orderPaths();
    const nextFocus = focusAfterRemoval(
      order,
      targets.map((target) => target.path),
    );
    const subject = single ? `“${single.name}”` : `${targets.length} items`;
    const bucketSubject = single ? `bucket “${single.name}”` : `${targets.length} buckets`;
    confirmation.ask({
      title: bucket
        ? `Delete ${bucketSubject}?`
        : irreversible
          ? `Permanently delete ${subject}?`
          : `Move ${subject} to the Trash?`,
      description: bucket
        ? "Only empty buckets can be deleted. This can’t be undone."
        : !irreversible
          ? single
            ? "You can restore it later from the Trash."
            : "You can restore them later from the Trash."
          : !single
            ? "The items and everything inside them are deleted from storage. This can’t be undone."
            : single.is_directory
              ? "The folder and everything inside it are deleted from storage. This can’t be undone."
              : "The file is deleted from storage. This can’t be undone.",
      confirmLabel: irreversible ? "Delete" : "Move to Trash",
      pendingLabel: irreversible ? "Deleting…" : "Moving…",
      globalHoldKeys: ["Delete", "Backspace"],
      // Start the deletion and let the sheet close immediately; progress lives
      // in the toast instead of the "Deleting…" pending state.
      onconfirm: () => {
        const paths = targets.map((target) => target.path);
        void (async () => {
          try {
            if (bucket) {
              for (const path of paths) await deleteRemoteBucket(path);
            } else if (remote) {
              await deleteRemoteItems(paths);
            } else {
              const label = `${irreversible ? "Delete" : "Move to Trash"}: ${subject}`;
              if (irreversible) await deleteItems(paths, label);
              else await trashItems(paths, label);
            }
          } catch (error) {
            await this.refreshListing(folder);
            if (remote || bucket)
              await this.showError(`Couldn’t delete ${targetLabel(targets)}`, error);
            else this.listingError = error instanceof Error ? error.message : String(error);
            return;
          }
          this.removeEntries(paths);
          if (nextFocus && this.selection.paths.length === 0) {
            this.setSelection({ paths: [], anchor: nextFocus, focus: nextFocus });
          }
        })();
      },
    });
  }

  openParent() {
    if (!this.isBrowsableFolder()) return;
    const path = parentPath(this.listingPath);
    if (!path || path === this.listingPath) return;
    // The root of an SFTP or FTP location has no parent inside the app.
    if (isServerPath(this.listingPath) && !isServerPath(path)) return;
    // A mounted share of an SMB server goes back to the server's share list.
    const server = networkStatus.serverOfShare(this.listingPath);
    if (server) {
      this.openLocation(server);
      return;
    }
    const name = toS3Uri(path).split("/").filter(Boolean).at(-1) ?? path;
    this.openLocation({ name, path, kind: "folder" });
  }

  async downloadContextTargets() {
    const targets = this.contextTargets;
    if (targets.length === 0) return;
    const destination = await open({ directory: true, title: "Download to…" });
    if (typeof destination !== "string") return;
    try {
      await downloadRemoteItems(
        targets.map((target) => target.path),
        destination,
      );
    } catch (error) {
      await this.showError(`Couldn’t download ${targetLabel(targets)}`, error);
    }
  }

  async uploadToListing(sources: string[]) {
    const folder = this.listingPath;
    try {
      await uploadRemoteFiles(folder, sources);
    } catch (error) {
      await this.showError("Upload failed", error);
    }
    await this.refreshListing(folder);
  }

  async chooseFilesToUpload() {
    const selection = await open({ multiple: true, title: "Upload" });
    const sources = Array.isArray(selection) ? selection : selection ? [selection] : [];
    if (sources.length) await this.uploadToListing(sources);
  }

  cancelRename() {
    this.renamingPath = "";
  }

  /** Right-click on a selected entry acts on the whole selection; on another entry it selects only that one. */
  handleContextMenu(entry: DirectoryEntry) {
    if (this.selectedSet.has(entry.path)) {
      this.contextTargets = this.selectedEntries;
      this.tabs.update({ selection: { ...this.selection, focus: entry.path } });
      return;
    }
    this.contextTargets = [entry];
    // Highlight without previewing, like before multiple selection.
    this.tabs.update({ selection: selectOnly(entry.path) });
  }

  async refreshOpenWithApps() {
    const target = this.contextTarget;
    this.openWithApps = [];
    this.defaultApp = null;
    if (!target || isRemoteLike(target.path)) return;
    const [apps, fallback] = await Promise.all([
      fetchOpenWithApps(target.path).catch(() => []),
      target.is_directory ? null : fetchDefaultApp(target.path).catch(() => null),
    ]);
    if (this.contextTarget !== target) return;
    this.openWithApps = fallback
      ? [fallback, ...apps.filter((app) => app.path !== fallback.path)]
      : apps;
    this.defaultApp = fallback;
  }

  onContextMenuOpenChange(open: boolean) {
    this.contextMenuOpen = open;
    if (open) {
      void this.refreshOpenWithApps();
    } else {
      this.contextTargets = [];
      this.openWithApps = [];
      this.defaultApp = null;
    }
  }

  openContextTargetInNewTab() {
    if (!this.contextTarget?.is_directory) return;
    if (this.contextTarget.kind === "share")
      return this.openEntry(this.contextTarget, { newTab: true });
    this.openLocation(
      { name: this.contextTarget.name, path: this.contextTarget.path, kind: "folder" },
      { newTab: true },
    );
  }

  openContextTarget() {
    if (!this.contextTarget) return;
    if (this.contextTarget.kind === "share") return this.openEntry(this.contextTarget);
    if (isRemoteLike(this.contextTarget.path) && this.contextTarget.is_directory) {
      this.openEntryInApp(this.contextTarget);
      return;
    }
    void this.openPath(this.contextTarget.path);
  }

  /** Opens an entry inside the app: folders navigate in, files jump to their containing
   *  folder and get selected so the preview pane shows them. */
  openEntryInApp(entry: DirectoryEntry) {
    if (entry.is_directory) {
      this.openLocation({ name: entry.name, path: entry.path, kind: "folder" });
      return;
    }
    const parent = parentPath(entry.path) || entry.path;
    this.openLocation({
      name: baseName(parent) || parent,
      path: parent,
      kind: "folder",
    });
    this.selectEntry(entry);
    if (!isRemoteLike(entry.path)) {
      void invoke("record_recent", { path: entry.path, name: entry.name, kind: "file" }).catch(
        () => {},
      );
    }
  }

  openContextTargetInApp() {
    if (this.contextTarget) this.openEntryInApp(this.contextTarget);
  }

  openContextTargetLocationInApp() {
    const entry = this.contextTarget;
    if (!entry) return;
    const parent = parentPath(entry.path) || entry.path;
    this.openLocation({
      name: baseName(parent) || parent,
      path: parent,
      kind: "folder",
    });
    this.selectEntry(entry);
  }

  openContextTargetInFinder() {
    if (this.contextTarget) void revealPath(this.contextTarget.path);
  }

  openWithContextTarget(appPath: string) {
    if (this.contextTarget) void openWithTarget(this.contextTarget.path, appPath);
  }

  async openWithOtherApplication() {
    const path = this.contextTarget?.path;
    if (!path) return;
    const appPath = await chooseApplication();
    if (appPath) await openWithTarget(path, appPath);
  }

  copyContextPaths() {
    if (this.contextTargets.length === 0) return;
    // Files inside a mount copy their local path, which other apps can open.
    const copied = this.contextTargets
      .map(({ path }) => (isServerPath(path) ? this.displayPath(path) : toS3Uri(path)))
      .join("\n");
    void navigator.clipboard.writeText(copied);
  }

  async duplicateContextTargets() {
    const targets = this.contextTargets;
    for (const target of targets) {
      try {
        if (isRemoteLike(target.path)) await copyItem(target.path, parentPath(target.path));
        else await copyItems([target.path], parentPath(target.path), `Duplicate: ${target.name}`);
      } catch (error) {
        if (isRemoteLike(target.path))
          void this.showError(`Couldn’t duplicate ${target.name}`, error);
        else this.listingError = error instanceof Error ? error.message : String(error);
        break;
      }
    }
    if (targets.length && this.isBrowsableFolder()) await this.refreshListing(this.listingPath);
  }

  async moveContextTargetsTo() {
    await this.transferContextTargets("Move to…", "move");
  }

  async copyContextTargetsTo() {
    await this.transferContextTargets("Copy to…", "copy");
  }

  private async transferContextTargets(title: string, mode: "copy" | "move") {
    const targets = this.contextTargets;
    if (targets.length === 0) return;
    const destination = await open({ directory: true, title, defaultPath: this.listingPath });
    if (typeof destination !== "string") return;
    const paths = targets.map((target) => target.path);
    const label = `${mode === "move" ? "Move" : "Copy"}: ${paths.length} items`;
    try {
      if (targets.some((target) => isRemoteLike(target.path))) {
        for (const path of paths) {
          if (mode === "move") await moveItem(path, destination);
          else await copyItem(path, destination);
        }
      } else if (mode === "move") {
        await moveItems(paths, destination, label);
      } else {
        await copyItems(paths, destination, label);
      }
    } catch (error) {
      this.listingError = error instanceof Error ? error.message : String(error);
    }
    if (this.isBrowsableFolder()) await this.refreshListing(this.listingPath);
  }

  async openCompressDialog() {
    const targets = this.contextTargets;
    if (targets.length === 0 || targets.some((target) => isRemoteLike(target.path))) return;
    if (!this.#sevenZipLoaded) {
      try {
        this.sevenZipPath = await detectSevenZip();
      } catch {
        this.sevenZipPath = null;
      }
      this.#sevenZipLoaded = true;
    }
    this.compressTargets = targets;
    this.compressDialogOpen = true;
  }

  async createCompressed(name: string, format: CompressFormat) {
    const targets = this.compressTargets;
    if (targets.length === 0) return;
    const parent = parentPath(targets[0].path) || targets[0].path;
    const destination = `${parent}${name}${compressExtension(format)}`;
    this.compressDialogOpen = false;
    try {
      await createArchive(
        targets.map((target) => target.path),
        destination,
      );
    } catch (error) {
      await this.showError(`Couldn’t compress ${targetLabel(targets)}`, error);
      return;
    }
    await this.refreshListing(this.listingPath);
  }

  extractContextTargetHere() {
    const target = this.contextTarget;
    if (!target || isRemoteLike(target.path)) return;
    void extraction.run({
      archive: target.path,
      destination: parentPath(target.path) || target.path,
    });
  }

  async extractContextTargetTo() {
    const target = this.contextTarget;
    if (!target || isRemoteLike(target.path)) return;
    const destination = await open({
      directory: true,
      title: "Extract to…",
      defaultPath: parentPath(target.path) || target.path,
    });
    if (typeof destination !== "string") return;
    void extraction.run({ archive: target.path, destination });
  }

  goBack() {
    this.scrollPositions.delete(this.tabs.activeId);
    this.tabs.back();
  }

  goForward() {
    this.scrollPositions.delete(this.tabs.activeId);
    this.tabs.forward();
  }

  /** A trackpad swipe moved: follow the finger. Amount is -1..1, positive is back. */
  updateSwipe(amount: number) {
    if (amount === 0 || this.keyboardBlocked()) return;
    this.swipeActive = true;
    this.swipeSettling = false;
    this.swipeAmount = amount;
    this.swipeDirection = amount > 0 ? "back" : "forward";
  }

  /** The swipe ended: commit the navigation or spring back. */
  finishSwipe({ committed }: { committed: boolean }) {
    if (!this.swipeActive) return;
    const direction = this.swipeDirection;
    if (committed && direction) {
      if (direction === "back") this.goBack();
      else this.goForward();
      this.#endSwipe();
      return;
    }
    this.swipeSettling = true;
    this.swipeAmount = 0;
    const token = ++this.#swipeToken;
    setTimeout(() => {
      if (token === this.#swipeToken) this.#endSwipe();
    }, 260);
  }

  #endSwipe() {
    this.#swipeToken++;
    this.swipeActive = false;
    this.swipeSettling = false;
    this.swipeAmount = 0;
    this.swipeDirection = null;
  }

  /** Keys that act on the file list wait while renaming, a context menu or a confirmation is open. */
  keyboardBlocked(): boolean {
    return Boolean(this.renamingPath) || this.contextMenuOpen || confirmation.open;
  }

  keyboardView(): "list" | "grid" {
    return this.navigator?.view() ?? this.viewMode;
  }

  /** Entries in the order keyboard navigation follows: what the mounted list shows. */
  private keyboardOrder(showHidden: boolean): DirectoryEntry[] {
    return (
      this.navigator?.order() ??
      this.sourceEntries.filter((entry) => showHidden || !entry.is_hidden)
    );
  }

  private focusedEntry(showHidden: boolean): DirectoryEntry | undefined {
    return this.keyboardOrder(showHidden).find((entry) => entry.path === this.focusPath);
  }

  private navLayout(count: number): NavLayout {
    return {
      count,
      columns: this.navigator?.columns() ?? 1,
      pageRows: this.navigator?.pageRows() ?? 10,
      view: this.keyboardView(),
    };
  }

  /** Focuses `paths[index]`. In visual mode the range from the anchor is selected instead. */
  private moveTo(paths: string[], index: number, modifiers: { shift: boolean; primary: boolean }) {
    const target = paths[index];
    this.setSelection(
      this.visual
        ? visualRange(paths, this.visual, target)
        : applyNav(this.selection, paths, target, modifiers),
    );
    this.navigator?.scrollToIndex(index);
  }

  /** Windows Explorer rules: Shift extends the range, the primary modifier moves focus only. */
  moveFocus(
    key: NavKey,
    modifiers: { shift: boolean; primary: boolean },
    showHidden: boolean,
  ): boolean {
    if (this.selected === "Overview") return false;
    const paths = this.keyboardOrder(showHidden).map((entry) => entry.path);
    const index = navTarget(paths.indexOf(this.focusPath), key, this.navLayout(paths.length));
    if (index === null) return false;
    this.moveTo(paths, index, modifiers);
    return true;
  }

  moveHalfPage(direction: 1 | -1, showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    const paths = this.keyboardOrder(showHidden).map((entry) => entry.path);
    const index = halfPageTarget(
      paths.indexOf(this.focusPath),
      direction,
      this.navLayout(paths.length),
    );
    if (index === null) return false;
    this.moveTo(paths, index, { shift: false, primary: false });
    return true;
  }

  /** Opens the focused entry: folders in place, files with their default app. */
  enterOrOpenFocused(showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    const entry = this.focusedEntry(showHidden);
    if (!entry) return false;
    this.openEntry(entry);
    return true;
  }

  toggleFocusedAndNext(showHidden: boolean): boolean {
    this.visual = null;
    if (!this.selectFocused("toggle", showHidden)) return false;
    this.moveFocus("ArrowDown", { shift: false, primary: true }, showHidden);
    return true;
  }

  invertSelection(showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    this.setSelection(
      invert(
        this.selection,
        this.keyboardOrder(showHidden).map((entry) => entry.path),
      ),
    );
    return true;
  }

  startVisual(mode: "add" | "remove", showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    const paths = this.keyboardOrder(showHidden).map((entry) => entry.path);
    const anchor = paths.includes(this.focusPath) ? this.focusPath : paths[0];
    if (!anchor) return false;
    this.visual = { mode, anchor, base: this.selection.paths };
    this.setSelection(visualRange(paths, this.visual, anchor));
    return true;
  }

  exitVisual(): boolean {
    if (!this.visual) return false;
    this.visual = null;
    return true;
  }

  renameFocused(showHidden: boolean): boolean {
    if (this.selected === "Overview" || this.remoteRoot || this.serverRoot) return false;
    const entry =
      this.focusedEntry(showHidden) ??
      (this.selectedEntries.length === 1 ? this.selectedEntries[0] : undefined);
    if (!entry) return false;
    this.renamingPath = entry.path;
    return true;
  }

  createFromKeyboard(kind: CreateKind): boolean {
    if (!this.isBrowsableFolder()) return false;
    void this.createItem(kind);
    return true;
  }

  /** Copies paths or names of the selection (or the focused entry), one per line. */
  copyText(kind: CopyTextKind, showHidden: boolean): boolean {
    const shown = (path: string) => (isServerPath(path) ? this.displayPath(path) : toS3Uri(path));
    let text: string;
    if (kind === "dirPath") {
      if (!this.listingPath) return false;
      text = shown(this.listingPath);
    } else {
      const focused = this.focusedEntry(showHidden);
      const targets =
        this.selectedEntries.length > 0 ? this.selectedEntries : focused ? [focused] : [];
      if (targets.length === 0) return false;
      text = targets
        .map((entry) =>
          kind === "path" ? shown(entry.path) : kind === "name" ? entry.name : fileStem(entry.name),
        )
        .join("\n");
    }
    void navigator.clipboard
      .writeText(text)
      .catch((error: unknown) => this.showError("Couldn’t copy", error));
    return true;
  }

  sortFromKeyboard(column: SortColumn, dir: SortDir): boolean {
    if (!this.navigator) return false;
    this.navigator.sort(column, dir);
    return true;
  }

  /** Opens the focused folder; false when focus is not on a folder. */
  enterFocused(showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    const entry = this.focusedEntry(showHidden);
    if (!entry?.is_directory) return false;
    this.openEntry(entry);
    return true;
  }

  selectFocused(mode: "only" | "toggle", showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    const entry = this.focusedEntry(showHidden);
    if (!entry) return false;
    this.setSelection(
      mode === "toggle" ? toggle(this.selection, entry.path) : selectOnly(entry.path),
    );
    return true;
  }

  selectAllListed(showHidden: boolean): boolean {
    if (this.selected === "Overview") return false;
    this.setSelection(selectAll(this.keyboardOrder(showHidden).map((entry) => entry.path)));
    return true;
  }

  clearSelectionIfAny(): boolean {
    if (this.selected === "Overview" || this.selection.paths.length === 0) return false;
    this.clearSelection();
    return true;
  }

  trashSelection({ permanent }: { permanent: boolean }): boolean {
    if (this.selected === "Overview") return false;
    const entries = this.selectedEntries;
    if (entries.length === 0) return false;
    this.deleteEntries(entries, { permanent });
    return true;
  }

  /** Opens the selected entries. With several selected, folders open in new tabs. */
  openSelection(): boolean {
    const entries = this.selectedEntries;
    if (entries.length === 0) return false;
    if (entries.length === 1) this.openEntry(entries[0]);
    else for (const entry of entries) this.openEntry(entry, { newTab: entry.is_directory });
    return true;
  }

  openParentFromKeyboard(): boolean {
    if (this.selected === "Overview") return false;
    this.openParent();
    return true;
  }

  openCreatedBucket({ bucket, warning }: CreatedBucket) {
    void this.refreshListing(this.listingPath);
    this.openLocation({ name: bucket.name, path: bucket.path, kind: "folder" });
    if (warning) void message(warning, { title: "Bucket created with warnings", kind: "warning" });
  }
}
