import { invoke } from "@tauri-apps/api/core";
import { message, open, save } from "@tauri-apps/plugin-dialog";
import { createArchive, extractArchive } from "$lib/file-ops/archive.js";
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
import { baseName, copyItem, deleteItem, moveItem, parentPath, trashItem } from "$lib/file-ops/files.js";
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

type Recent = { name: string; path: string; kind: string; opened_at: number };

/** S3 objects and SFTP/FTP files: changed through backend commands and opened from a cached download. */
function isRemoteLike(path: string): boolean {
  return isRemotePath(path) || isServerPath(path);
}
export class FilePaneController {
  entries = $state<DirectoryEntry[]>([]);
  recents = $state<Recent[]>([]);
  listing = $state(false);
  listingError = $state("");
  /** Saved network location whose share or session is gone, shown with a Reconnect button. */
  disconnected = $state<Location | null>(null);
  renamingPath = $state("");
  contextTarget = $state<DirectoryEntry | null>(null);
  previewEntryPath = $state("");
  previewOpen = $state(true);
  contextMenuOpen = $state(false);
  remoteDropActive = $state(false);
  openWithApps = $state<OpenWithApp[]>([]);
  defaultApp = $state<OpenWithApp | null>(null);
  /** Set by the app shell so both panes feed the same transfer clipboard. */
  onTransferClipboard: (entry: QueueEntry) => void = () => {};

  scrollPositions = new Map<string, number>();

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
  get selectedEntryPath() {
    return this.tabs.active.selectedEntryPath;
  }
  get selected() {
    return this.tabs.active.location.name;
  }

  constructor(
    readonly paneId: string,
    readonly tabs: TabsStore,
    private readonly onWindowClose: () => void,
  ) {}

  selectEntry(entry: DirectoryEntry) {
    this.tabs.update({ selectedEntryPath: entry.path });
    this.previewEntryPath = entry.path;
  }

  clearSelection() {
    this.tabs.update({ selectedEntryPath: "" });
    this.previewEntryPath = "";
  }

  enqueueSelected(mode: ClipboardMode): boolean {
    if (this.serverRoot || !this.selectedEntryPath) return false;
    const entries = this.selected === "Recents" ? this.recentEntries : this.entries;
    const entry = entries.find((candidate) => candidate.path === this.selectedEntryPath);
    if (!entry) return false;
    this.onTransferClipboard({ path: entry.path, name: entry.name, isDirectory: entry.is_directory, mode });
    return true;
  }

  enqueueContextTarget(mode: ClipboardMode) {
    const entry = this.contextTarget;
    if (!entry || this.serverRoot) return;
    this.onTransferClipboard({ path: entry.path, name: entry.name, isDirectory: entry.is_directory, mode });
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
      });
    }
  }

  async loadLocation(location: Location) {
    const token = ++this.loadToken;
    this.renamingPath = "";
    this.listingError = "";
    this.disconnected = null;
    if (location.kind === "recents") {
      this.listing = false;
      this.entries = [];
      const result = await invoke<Recent[]>("recents");
      if (token === this.loadToken) this.recents = result;
      return;
    }
    if (!location.path) {
      this.listing = false;
      this.entries = [];
      return;
    }
    this.listing = true;
    try {
      const result = await invoke<DirectoryEntry[]>("read_directory", { path: location.path });
      if (token !== this.loadToken) return;
      this.entries = result;
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
    void invoke("record_recent", { path: entry.path, name: entry.name, kind: "file" });
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

  /** Enter opens the selected entry, unless focus is in a text field, menu or other control. */
  handleEnterKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.defaultPrevented || event.isComposing) return;
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
    if (this.renamingPath || this.contextMenuOpen || !this.selectedEntryPath) return;
    const target = event.target instanceof Element ? event.target : null;
    if (
      target?.closest(
        "input, textarea, select, [contenteditable='true'], [role='menu'], [role='dialog']",
      )
    )
      return;
    if (target?.closest("button, a") && !target.closest("[data-file-list]")) return;
    const list = this.selected === "Recents" ? this.recentEntries : this.entries;
    const entry = list.find((candidate) => candidate.path === this.selectedEntryPath);
    if (!entry) return;
    event.preventDefault();
    this.openEntry(entry);
  }

  /** Finder/Explorer-style selection and folder navigation for the active file pane. */
  handleFileListKeydown(event: KeyboardEvent, showHidden: boolean) {
    if (
      event.defaultPrevented ||
      event.isComposing ||
      this.renamingPath ||
      this.contextMenuOpen ||
      confirmation.open
    )
      return;
    const target = event.target instanceof Element ? event.target : null;
    if (
      target?.closest(
        "input, textarea, select, [contenteditable='true'], [role='menu'], [role='dialog'], button, a",
      )
    )
      return;

    const entries = (this.selected === "Recents" ? this.recentEntries : this.entries).filter(
      (entry) => showHidden || !entry.is_hidden,
    );
    const selectedIndex = entries.findIndex((entry) => entry.path === this.selectedEntryPath);

    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (event.metaKey || event.ctrlKey || event.altKey || entries.length === 0) return;
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const index =
        selectedIndex < 0
          ? delta > 0
            ? 0
            : entries.length - 1
          : Math.max(0, Math.min(entries.length - 1, selectedIndex + delta));
      this.selectEntry(entries[index]);
      return;
    }

    if (event.key === "ArrowRight" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const entry = entries[selectedIndex];
      if (!entry?.is_directory) return;
      event.preventDefault();
      this.openEntry(entry);
      return;
    }

    if (event.key === "ArrowLeft" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      this.openParent();
      return;
    }

    if ((event.key === "Delete" || event.key === "Backspace") && !event.altKey && !event.ctrlKey) {
      const entry = entries[selectedIndex];
      if (!entry) return;
      event.preventDefault();
      this.deleteEntry(entry, { permanent: event.shiftKey || event.metaKey });
    }
  }

  applyDirectorySizes(payload: { path: string; sizes: { path: string; size: number }[] }) {
    if (payload.path !== this.listingPath) return;
    for (const item of payload.sizes) {
      const index = this.entryIndex.get(item.path);
      if (index !== undefined) this.entries[index].size = item.size;
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
      this.entries = await invoke<DirectoryEntry[]>("read_directory", { path: this.listingPath });
      this.selectEntry(entry);
      this.renamingPath = entry.path;
    } catch (error) {
      if (this.remoteListing) return this.showError(`Couldn’t create ${name}`, error);
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  async renameItem(oldPath: string, newName: string) {
    try {
      await invoke("rename_item", { path: oldPath, newName });
    } catch (error) {
      if (isRemoteLike(oldPath)) void this.showError("Couldn’t rename", error);
      else this.listingError = error instanceof Error ? error.message : String(error);
    } finally {
      if (this.isBrowsableFolder()) {
        this.entries = await invoke<DirectoryEntry[]>("read_directory", { path: this.listingPath });
      }
      this.renamingPath = "";
    }
  }

  async showError(title: string, error: unknown) {
    await message(error instanceof Error ? error.message : String(error), { title, kind: "error" });
  }

  async refreshListing(path: string) {
    if (this.listingPath !== path || !path || this.selected === "Recents") return;
    try {
      const result = await invoke<DirectoryEntry[]>("read_directory", { path });
      if (this.listingPath === path) this.entries = result;
    } catch (error) {
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  renameContextTarget() {
    if (this.contextTarget) this.renamingPath = this.contextTarget.path;
  }

  deleteContextTarget() {
    if (this.contextTarget) this.deleteEntry(this.contextTarget, { permanent: true });
  }

  deleteEntry(target: DirectoryEntry, { permanent }: { permanent: boolean }) {
    const folder = this.listingPath;
    const bucket = this.remoteRoot;
    const remote = isRemoteLike(target.path);
    const irreversible = permanent || remote;
    confirmation.ask({
      title: bucket
        ? `Delete bucket “${target.name}”?`
        : irreversible
          ? `Permanently delete “${target.name}”?`
          : `Move “${target.name}” to the Trash?`,
      description: bucket
        ? "Only empty buckets can be deleted. This can’t be undone."
        : !irreversible
          ? "You can restore it later from the Trash."
          : target.is_directory
            ? "The folder and everything inside it are deleted from storage. This can’t be undone."
            : "The file is deleted from storage. This can’t be undone.",
      confirmLabel: irreversible ? "Delete" : "Move to Trash",
      pendingLabel: irreversible ? "Deleting…" : "Moving…",
      globalHoldKeys: ["Delete", "Backspace"],
      onconfirm: async () => {
        if (bucket) await deleteRemoteBucket(target.path);
        else if (remote) await deleteRemoteItems([target.path]);
        else if (irreversible) await deleteItem(target.path);
        else await trashItem(target.path);
        if (this.selectedEntryPath === target.path) this.clearSelection();
        await this.refreshListing(folder);
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

  async downloadContextTarget() {
    const target = this.contextTarget;
    if (!target) return;
    const destination = await open({ directory: true, title: "Download to…" });
    if (typeof destination !== "string") return;
    try {
      await downloadRemoteItems([target.path], destination);
    } catch (error) {
      await this.showError(`Couldn’t download ${target.name}`, error);
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

  handleContextMenu(entry: DirectoryEntry) {
    this.contextTarget = entry;
    this.tabs.update({ selectedEntryPath: entry.path });
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
      this.contextTarget = null;
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
      void invoke("record_recent", { path: entry.path, name: entry.name, kind: "file" });
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

  copyContextPath() {
    const path = this.contextTarget?.path;
    if (!path) return;
    // Files inside a mount copy their local path, which other apps can open.
    const copied = isServerPath(path) ? this.displayPath(path) : toS3Uri(path);
    void navigator.clipboard.writeText(copied);
  }

  async duplicateContextTarget() {
    const target = this.contextTarget;
    if (!target) return;
    const parent = parentPath(target.path);
    try {
      await copyItem(target.path, parent);
      if (this.isBrowsableFolder()) {
        this.entries = await invoke<DirectoryEntry[]>("read_directory", { path: this.listingPath });
      }
    } catch (error) {
      if (isRemoteLike(target.path))
        return this.showError(`Couldn’t duplicate ${target.name}`, error);
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  async moveContextTargetTo() {
    const target = this.contextTarget;
    if (!target) return;
    const destination = await open({
      directory: true,
      title: "Move to…",
      defaultPath: this.listingPath,
    });
    if (typeof destination !== "string") return;
    try {
      await moveItem(target.path, destination);
      if (this.isBrowsableFolder()) {
        this.entries = await invoke<DirectoryEntry[]>("read_directory", { path: this.listingPath });
      }
    } catch (error) {
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  async copyContextTargetTo() {
    const target = this.contextTarget;
    if (!target) return;
    const destination = await open({
      directory: true,
      title: "Copy to…",
      defaultPath: this.listingPath,
    });
    if (typeof destination !== "string") return;
    try {
      await copyItem(target.path, destination);
      if (this.isBrowsableFolder()) {
        this.entries = await invoke<DirectoryEntry[]>("read_directory", { path: this.listingPath });
      }
    } catch (error) {
      this.listingError = error instanceof Error ? error.message : String(error);
    }
  }

  async compressContextTarget() {
    const target = this.contextTarget;
    if (!target || isRemoteLike(target.path)) return;
    const parent = parentPath(target.path) || target.path;
    const destination = await save({
      title: "Compress",
      defaultPath: `${parent}${target.name}.zip`,
    });
    if (typeof destination !== "string") return;
    try {
      await createArchive([target.path], destination);
    } catch (error) {
      await this.showError(`Couldn’t compress ${target.name}`, error);
    }
    await this.refreshListing(this.listingPath);
  }

  async extractContextTargetHere() {
    const target = this.contextTarget;
    if (!target || isRemoteLike(target.path)) return;
    const parent = parentPath(target.path) || target.path;
    try {
      await extractArchive(target.path, parent);
    } catch (error) {
      await this.showError(`Couldn’t extract ${target.name}`, error);
    }
    await this.refreshListing(this.listingPath);
  }

  async extractContextTargetTo() {
    const target = this.contextTarget;
    if (!target || isRemoteLike(target.path)) return;
    const fallback = parentPath(target.path) || target.path;
    const destination = await open({
      directory: true,
      title: "Extract to…",
      defaultPath: fallback,
    });
    if (typeof destination !== "string") return;
    try {
      await extractArchive(target.path, destination);
    } catch (error) {
      await this.showError(`Couldn’t extract ${target.name}`, error);
    }
    await this.refreshListing(this.listingPath);
  }

  goBack() {
    this.scrollPositions.delete(this.tabs.activeId);
    this.tabs.back();
  }

  goForward() {
    this.scrollPositions.delete(this.tabs.activeId);
    this.tabs.forward();
  }

  openCreatedBucket({ bucket, warning }: CreatedBucket) {
    void this.refreshListing(this.listingPath);
    this.openLocation({ name: bucket.name, path: bucket.path, kind: "folder" });
    if (warning) void message(warning, { title: "Bucket created with warnings", kind: "warning" });
  }
}
