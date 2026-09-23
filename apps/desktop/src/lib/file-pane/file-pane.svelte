<script lang="ts">
  import FileInfoDialog from "$lib/components/custom/preview/file-info-dialog.svelte";
  let infoOpen = $state(false);
  let infoEntry = $state<DirectoryEntry | null>(null);
  function showInfo(entry: DirectoryEntry) { infoEntry = { ...entry }; infoOpen = true; }
  import CalculatorIcon from "@lucide/svelte/icons/calculator";
  import { onDestroy } from "svelte";
  import Clock3Icon from "@lucide/svelte/icons/clock-3";
  import ExternalLinkIcon from "@lucide/svelte/icons/external-link";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import FolderInputIcon from "@lucide/svelte/icons/folder-input";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import FolderPlusIcon from "@lucide/svelte/icons/folder-plus";
  import FilePlusIcon from "@lucide/svelte/icons/file-plus";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import CopyPlusIcon from "@lucide/svelte/icons/copy-plus";
  import ScissorsIcon from "@lucide/svelte/icons/scissors";
  import SquarePlusIcon from "@lucide/svelte/icons/square-plus";
  import StarIcon from "@lucide/svelte/icons/star";
  import StarOffIcon from "@lucide/svelte/icons/star-off";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import UploadIcon from "@lucide/svelte/icons/upload";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import DatabaseIcon from "@lucide/svelte/icons/database";
  import Settings2Icon from "@lucide/svelte/icons/settings-2";
  import AppWindowIcon from "@lucide/svelte/icons/app-window";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import FolderOutputIcon from "@lucide/svelte/icons/folder-output";
  import UnplugIcon from "@lucide/svelte/icons/unplug";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import InfoIcon from "@lucide/svelte/icons/info";
  import type { Location } from "$lib/tabs/tabs.js";
  import { networkStatus } from "$lib/remote/network-status.svelte.js";
  import { isArchive } from "$lib/file-ops/archive.js";
  import { drag } from "$lib/file-drag/drag.svelte.js";
  import { isNetworkPath } from "$lib/remote/network-locations.js";
  import { canFavorite } from "$lib/favorites/favorites.js";
  import PathStatusBar from "$lib/components/custom/status-bar/path-status-bar.svelte";
  import CompressDialog from "$lib/components/custom/dialog/compress-dialog.svelte";
  import { ListPanel, type DirectoryEntry } from "$lib/components/custom/file-list/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import { folderScan, OverviewPanel } from "$lib/components/custom/overview/index.js";
  import { TabBar } from "$lib/components/custom/tabs/index.js";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import { swipeTransforms } from "$lib/swipe/gesture.js";
  import type { FilePaneController } from "./controller.svelte.js";

  let {
    controller,
    active = false,
    onActivate = () => {},
    showHiddenFiles = false,
    itemCheckboxes = false,
    onEnableCheckboxes = () => {},
    showFps = false,
    revealLabel = "Show in Finder",
    transferActivity = null,
    onOpenBucketSettings = (_entry: DirectoryEntry) => {},
    onNewBucket = () => {},
    onReconnect = (_location: Location) => {},
    onExternalDrop = (_paths: string[], _options: { move: boolean }) => {},
    onCrossPaneDrop = (_from: string, _to: string, _fromIndex: number, _toIndex: number) => {},
    favoritePaths = new Set<string>(),
    pastedPaths = new Set<string>(),
    onToggleFavorite = (_entry: DirectoryEntry, _add: boolean) => {},
    keyboardMode = "standard",
    pendingKeys = "",
  }: {
    controller: FilePaneController;
    active?: boolean;
    onActivate?: () => void;
    showHiddenFiles?: boolean;
    /** Shows selection checkboxes in the file list. */
    itemCheckboxes?: boolean;
    onEnableCheckboxes?: () => void;
    showFps?: boolean;
    revealLabel?: string;
    transferActivity?: string | null;
    onOpenBucketSettings?: (entry: DirectoryEntry) => void;
    onNewBucket?: () => void;
    onReconnect?: (location: Location) => void;
    onExternalDrop?: (paths: string[], options: { move: boolean }) => void;
    onCrossPaneDrop?: (fromPaneId: string, toPaneId: string, fromIndex: number, toIndex: number) => void;
    /** Paths of the sidebar favorites, to offer adding or removing a folder. */
    favoritePaths?: Set<string>;
    pastedPaths?: Set<string>;
    onToggleFavorite?: (entry: DirectoryEntry, add: boolean) => void;
    keyboardMode?: "standard" | "yazi";
    pendingKeys?: string;
  } = $props();

  onDestroy(() => controller.cancelSizeScan());

  let statusPath = $derived(controller.selected === "Overview" ? "" : controller.listingPath);

  // Lets archive entries dragged from any preview extract into this pane.
  $effect(() => {
    const id = controller.paneId;
    const localListing = () =>
      controller.selected !== "Overview" &&
      controller.selected !== "Recents" &&
      !!controller.listingPath &&
      !controller.remoteListing &&
      !isNetworkPath(controller.listingPath);
    const handle = {
      folder: () => (localListing() ? controller.listingPath : null),
      isFolder: (path: string) =>
        localListing() && controller.entries.some((entry) => entry.path === path && entry.is_directory && !entry.kind),
    };
    drag.paneFolders.set(id, handle);
    return () => {
      if (drag.paneFolders.get(id) === handle) drag.paneFolders.delete(id);
    };
  });
  let reconnecting = $derived(
    controller.disconnected ? networkStatus.status(controller.disconnected.path).state === "connecting" : false,
  );

  let swipe = $derived(swipeTransforms(controller.swipeAmount, controller.canBack, controller.canForward));
  let incoming = $derived(
    controller.swipeActive && controller.swipeDirection
      ? controller.swipeTarget(controller.swipeDirection)
      : null,
  );
</script>

<FileInfoDialog bind:open={infoOpen} entry={infoEntry} onChanged={() => void controller.refreshListing(controller.listingPath)} />

{#snippet favoriteItem(target: DirectoryEntry)}
  {#if canFavorite(target)}
    {#if favoritePaths.has(target.path)}
      <ContextMenu.Item onSelect={() => onToggleFavorite(target, false)}>
        <StarOffIcon class="size-4" />
        Remove from Favorites
      </ContextMenu.Item>
    {:else}
      <ContextMenu.Item onSelect={() => onToggleFavorite(target, true)}>
        <StarIcon class="size-4" />
        Add to Favorites
      </ContextMenu.Item>
    {/if}
  {/if}
{/snippet}

<section class="file-pane" aria-label="File pane" class:active onmouseenter={onActivate}>
  <TabBar
    tabs={controller.tabs.tabs}
    activeId={controller.tabs.activeId}
    paneId={controller.paneId}
    onSelect={(id) => controller.tabs.select(id)}
    onClose={(id) => controller.closeTab(id)}
    onMove={(from, to) => controller.tabs.move(from, to)}
    onCrossPaneDrop={onCrossPaneDrop}
    onNew={() => controller.tabs.open()} />

  <div
    class="swipe-stage"
    class:swiping={controller.swipeActive}
    class:settling={controller.swipeSettling}>
    {#if controller.swipeActive && controller.swipeDirection}
      <div class="swipe-pane incoming" style:transform={`translate3d(${swipe.incoming}%, 0, 0)`}>
        {#if incoming?.entries}
          <div class="swipe-incoming-fill" inert>
            <ListPanel
              previewing
              entries={incoming.entries}
              view={controller.viewMode}
              showHidden={showHiddenFiles}
              sortKey={incoming.path}
              previewOpen={false}
              scrollTop={0} />
          </div>
        {:else}
          <section class="finder-empty" aria-live="polite"><p>Loading…</p></section>
        {/if}
      </div>
    {/if}
    <div class="swipe-pane current" style:transform={`translate3d(${swipe.current}%, 0, 0)`}>
  {#if controller.selected === "Overview"}
    <OverviewPanel
      showHidden={showHiddenFiles}
      onOpen={(entry) => controller.openLocation({ name: entry.name, path: entry.path, kind: "folder" })} />
  {:else if controller.listing}
    <section class="finder-empty" aria-live="polite"><p>Loading {controller.selected}…</p></section>
  {:else if controller.disconnected}
    <section class="finder-empty network-disconnected" aria-live="polite">
      <UnplugIcon />
      <h1>“{controller.disconnected.name}” is disconnected</h1>
      <p>The share was ejected or the server connection ended.</p>
      <button
        class="reconnect-button"
        disabled={reconnecting}
        onclick={() => controller.disconnected && onReconnect(controller.disconnected)}
        >{reconnecting ? "Reconnecting…" : "Reconnect"}</button>
    </section>
  {:else if controller.listingError}
    <section class="finder-empty" aria-live="polite">
      <FolderIcon />
      <h1>Can’t open {controller.selected}</h1>
      <p>{controller.listingError}</p>
    </section>
  {:else if controller.selected === "Recents" && controller.recentEntries.length}
    <div class="flex min-h-0 flex-1 flex-col">
      <div class="flex items-center justify-end px-3 pt-2">
        <button
          class="flex items-center gap-1.5 rounded-md border-0 bg-transparent px-2 py-1 text-[13px] text-[#9c9895] hover:bg-[#353230] hover:text-[#e8e5e2] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#0a9bff]"
          aria-label="Clear recents history"
          onclick={() => void controller.clearRecents()}><Trash2Icon class="size-4" /> Clear history</button>
      </div>
      <ContextMenu.Root bind:open={controller.contextMenuOpen} onOpenChange={(open) => controller.onContextMenuOpenChange(open)}>
        <ContextMenu.Trigger class="flex min-h-0 min-w-0 flex-1">
          <ListPanel
            entries={controller.recentEntries}
            sortKey="recents"
            view={controller.viewMode}
            showHidden={showHiddenFiles}
            paneId={controller.paneId}
            {pastedPaths}
            selectedPaths={controller.selectedSet}
            focusPath={controller.focusPath}
            checkboxes={itemCheckboxes}
            onItemClick={(entry, modifiers) => controller.clickEntry(entry, modifiers)}
            onToggle={(entry) => controller.toggleEntry(entry)}
            onRegisterNavigator={(navigator) => controller.registerNavigator(navigator)}
          onToggleAll={() => controller.toggleAll()}
          {onEnableCheckboxes}
            onClearSelection={() => controller.clearSelection()}
            onOpen={(entry, options) => controller.openEntry(entry, options)}
            onContextMenu={(entry) => controller.handleContextMenu(entry)}
            onExternalDrop={onExternalDrop}
            previewPath={controller.previewEntryPath}
            previewOpen={controller.previewOpen}
            scrollTop={controller.scrollPositions.get(controller.tabs.activeId) ?? 0}
            onScroll={(top: number) => controller.scrollPositions.set(controller.tabs.activeId, top)} />
        </ContextMenu.Trigger>
        <ContextMenu.Content>
          {#if controller.contextTargets.length > 1}
            <ContextMenu.Item onSelect={() => controller.copyContextPaths()}>
              <CopyIcon class="size-4" />
              Copy {controller.contextTargets.length} Paths
            </ContextMenu.Item>
          {:else if controller.contextTarget}
            <ContextMenu.Item onSelect={() => showInfo(controller.contextTarget!)}>
              <InfoIcon class="size-4" />
              Get Info
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => controller.openContextTargetInApp()}>
              <FolderOpenIcon class="size-4" />
              {controller.contextTarget.is_directory ? "Open Folder" : "Open File"}
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => controller.openContextTargetLocationInApp()}>
              <FolderInputIcon class="size-4" />
              {controller.contextTarget.is_directory ? "Open Folder Location" : "Open File Location"}
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => controller.openContextTargetInFinder()}>
              <ExternalLinkIcon class="size-4" />
              {revealLabel}
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => controller.copyContextPaths()}>
              <CopyIcon class="size-4" />
              Copy Path
            </ContextMenu.Item>
            {@render favoriteItem(controller.contextTarget)}
          {/if}
        </ContextMenu.Content>
      </ContextMenu.Root>
    </div>
  {:else if controller.selected === "Recents"}
    <section class="finder-empty" aria-live="polite">
      <Clock3Icon />
      <h1>Recents</h1>
      <p>No recent items in the last 30 days.</p>
    </section>
  {:else if controller.listingPath}
    <ContextMenu.Root bind:open={controller.contextMenuOpen} onOpenChange={(open) => controller.onContextMenuOpenChange(open)}>
      <ContextMenu.Trigger class={`flex min-h-0 min-w-0 flex-1${controller.remoteDropActive ? " remote-drop-active" : ""}`}>
        <ListPanel
          onFilesChanged={() => void controller.refreshListing(controller.listingPath)}
          entries={controller.visibleEntries}
          searchQuery={controller.searchMode === "content" ? controller.searchQuery.trim() : ""}
          sortKey={controller.listingPath}
          view={controller.viewMode}
          showHidden={showHiddenFiles}
          paneId={controller.paneId}
          {pastedPaths}
          selectedPaths={controller.selectedSet}
          focusPath={controller.focusPath}
          checkboxes={itemCheckboxes}
          renamingPath={controller.renamingPath}
          onItemClick={(entry, modifiers) => controller.clickEntry(entry, modifiers)}
          onToggle={(entry) => controller.toggleEntry(entry)}
          onRegisterNavigator={(navigator) => controller.registerNavigator(navigator)}
          onToggleAll={() => controller.toggleAll()}
          {onEnableCheckboxes}
          onClearSelection={() => controller.clearSelection()}
          onOpen={(entry, options) => controller.openEntry(entry, options)}
          scrollTop={controller.scrollPositions.get(controller.tabs.activeId) ?? 0}
          onScroll={(top: number) => controller.scrollPositions.set(controller.tabs.activeId, top)}
          onContextMenu={(entry) => controller.handleContextMenu(entry)}
          onExternalDrop={onExternalDrop}
          previewPath={controller.previewEntryPath}
          previewOpen={controller.previewOpen}
          onReorder={(reordered: DirectoryEntry[]) => (controller.entries = reordered)}
          onRename={(oldPath, newName) => controller.renameItem(oldPath, newName)}
          onRenameCancel={() => controller.cancelRename()} />
      </ContextMenu.Trigger>
      <ContextMenu.Content onCloseAutoFocus={(event) => { if (controller.renamingPath) event.preventDefault(); }}>
        {#if controller.contextTargets.length}
          {@const target = controller.contextTarget}
          {@const count = controller.contextTargets.length}
          {#if target?.is_directory}
            <ContextMenu.Item onSelect={() => controller.openContextTargetInNewTab()}>
              <SquarePlusIcon class="size-4" />
              Open in New Tab
            </ContextMenu.Item>
            {@render favoriteItem(target)}
          {/if}
          {#if target}
            <ContextMenu.Item onSelect={() => showInfo(target)}>
              <InfoIcon class="size-4" />
              Get Info
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => controller.openContextTarget()}>
              <FolderOpenIcon class="size-4" />
              Open
              {#if controller.defaultApp}
                <ContextMenu.Shortcut class="flex max-w-24 min-w-0 items-center gap-1 pl-3 text-[9px]">
                  {#if controller.defaultApp.icon}<img src={controller.defaultApp.icon} alt="" class="size-4 shrink-0" draggable="false" />{/if}
                  <span class="min-w-0 truncate">{controller.defaultApp.name}</span>
                </ContextMenu.Shortcut>
              {/if}
            </ContextMenu.Item>
            {#if !controller.remoteListing && !controller.serverRoot}
              <ContextMenu.Sub>
                <ContextMenu.SubTrigger>
                  <AppWindowIcon class="size-4" />
                  Open With
                </ContextMenu.SubTrigger>
                <ContextMenu.SubContent>
                  {#if controller.openWithApps.length === 0}
                    <ContextMenu.Label>No applications</ContextMenu.Label>
                  {:else}
                    {#each controller.openWithApps as app (app.path)}
                      <ContextMenu.Item onSelect={() => controller.openWithContextTarget(app.path)}>
                        {#if app.icon}
                          <img src={app.icon} alt="" class="size-4 shrink-0" draggable="false" />
                        {:else}
                          <AppWindowIcon class="size-4" />
                        {/if}
                        {app.name}
                        {#if app.path === controller.defaultApp?.path}
                          <span class="text-white/40">(default)</span>
                        {/if}
                      </ContextMenu.Item>
                    {/each}
                  {/if}
                  <ContextMenu.Separator />
                  <ContextMenu.Item onSelect={() => void controller.openWithOtherApplication()}>Other…</ContextMenu.Item>
                </ContextMenu.SubContent>
              </ContextMenu.Sub>
            {/if}
          {/if}
          <ContextMenu.Item onSelect={() => controller.copyContextPaths()}>
            <CopyIcon class="size-4" />
            {count > 1 ? `Copy ${count} Paths` : "Copy Path"}
          </ContextMenu.Item>
          {#if !controller.serverRoot}
            <ContextMenu.Item onSelect={() => controller.enqueueContextTargets("copy")}>
              <CopyIcon class="size-4" />
              {count > 1 ? `Copy ${count} Items` : "Copy"}
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => controller.enqueueContextTargets("move")}>
              <ScissorsIcon class="size-4" />
              {count > 1 ? `Move ${count} Items` : "Move"}
            </ContextMenu.Item>
          {/if}
          {#if !controller.remoteRoot && !controller.serverRoot}
            <ContextMenu.Separator />
            {#if target}
              <ContextMenu.Item onSelect={() => controller.renameContextTarget()}>
                <PencilIcon class="size-4" />
                Rename
              </ContextMenu.Item>
            {/if}
            <ContextMenu.Item onSelect={() => void controller.duplicateContextTargets()}>
              <CopyPlusIcon class="size-4" />
              {count > 1 ? `Duplicate ${count} Items` : "Duplicate"}
            </ContextMenu.Item>
          {/if}
          {#if controller.serverRoot}
            <!-- Shares of an SMB server only open. -->
          {:else if !controller.remoteListing}
            <ContextMenu.Item onSelect={() => void controller.moveContextTargetsTo()}>
              <ScissorsIcon class="size-4" />
              Move to…
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => void controller.copyContextTargetsTo()}>
              <FolderInputIcon class="size-4" />
              Copy to…
            </ContextMenu.Item>
            <ContextMenu.Separator />
            <ContextMenu.Item onSelect={() => void controller.openCompressDialog()}>
              <ArchiveIcon class="size-4" />
              {target ? `Compress “${target.name}”…` : `Compress ${count} Items…`}
            </ContextMenu.Item>
            {#if target && isArchive(target.name)}
              <ContextMenu.Item onSelect={() => void controller.extractContextTargetHere()}>
                <FolderOutputIcon class="size-4" />
                Extract Here
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => void controller.extractContextTargetTo()}>
                <FolderInputIcon class="size-4" />
                Extract to…
              </ContextMenu.Item>
            {/if}
          {:else}
            <ContextMenu.Item onSelect={() => void controller.downloadContextTargets()}>
              <DownloadIcon class="size-4" />
              {count > 1 ? `Download ${count} Items…` : "Download…"}
            </ContextMenu.Item>
            {#if controller.remoteRoot && target}
              <ContextMenu.Item onSelect={() => onOpenBucketSettings(target)}>
                <Settings2Icon class="size-4" />
                Bucket Settings…
              </ContextMenu.Item>
            {/if}
            <ContextMenu.Item variant="destructive" onSelect={() => controller.deleteContextTargets()}>
              <Trash2Icon class="size-4" />
              {controller.remoteRoot
                ? count > 1 ? `Delete ${count} Buckets…` : "Delete Bucket…"
                : count > 1 ? `Delete ${count} Items…` : "Delete…"}
            </ContextMenu.Item>
          {/if}
        {/if}
        {#if !controller.contextTargets.length && controller.canCalculateSizes}
          <ContextMenu.Item onSelect={() => controller.sizeScanning ? controller.cancelSizeScan() : void controller.calculateSizes()}>
            <CalculatorIcon class="size-4" />
            {controller.sizeScanning ? "Cancel Size Calculation" : "Calculate Sizes"}
          </ContextMenu.Item>
        {/if}
        {#if controller.contextTargets.length || controller.canCalculateSizes}
          <ContextMenu.Separator />
        {/if}
        <ContextMenu.Item onSelect={() => controller.refresh()}>
          <RefreshCwIcon class="size-4" />
          Refresh
        </ContextMenu.Item>
        {#if !controller.remoteRoot && !controller.serverRoot}
          <ContextMenu.Item onSelect={() => void controller.createItem("folder")}>
            <FolderPlusIcon class="size-4" />
            New Folder
          </ContextMenu.Item>
          <ContextMenu.Item onSelect={() => void controller.createItem("file")}>
            <FilePlusIcon class="size-4" />
            New File
          </ContextMenu.Item>
        {/if}
        {#if controller.remoteRoot}
          <ContextMenu.Item onSelect={onNewBucket}>
            <DatabaseIcon class="size-4" />
            New Bucket…
          </ContextMenu.Item>
        {/if}
        {#if controller.remoteListing && !controller.remoteRoot}
          <ContextMenu.Item onSelect={() => void controller.chooseFilesToUpload()}>
            <UploadIcon class="size-4" />
            Upload Files…
          </ContextMenu.Item>
        {/if}
      </ContextMenu.Content>
    </ContextMenu.Root>
  {:else}
    <section class="finder-empty" aria-live="polite">
      <FolderIcon />
      <h1>{controller.selected}</h1>
      <p>Select a favorite or location to view its files.</p>
    </section>
  {/if}
    </div>
  </div>

  <PathStatusBar
    {showFps}
    sizeScanning={controller.sizeScanning}
    sizeScanMessage={controller.sizeScanMessage}
    onCancelSizeScan={() => controller.cancelSizeScan()}
    {keyboardMode}
    {pendingKeys}
    visual={Boolean(controller.visual)}
    path={statusPath ? controller.displayPath(statusPath) : ""}
    location={statusPath}
    onNavigate={(path) => controller.openBreadcrumb(path)}
    network={Boolean(statusPath && networkStatus.ownerOf(statusPath))}
    entries={controller.selected === "Recents"
      ? controller.recentEntries
      : controller.selected === "Overview"
        ? []
        : controller.entries}
    selectedEntries={controller.selected === "Overview" ? [] : controller.selectedEntries}
    activity={transferActivity ??
      (folderScan.scanning
        ? `Analyzing ${folderScan.rootName}… ${formatSize(folderScan.scannedBytes)}`
        : null)} />
  <CompressDialog
    bind:open={controller.compressDialogOpen}
    targets={controller.compressTargets}
    sevenZipPath={controller.sevenZipPath}
    onCreate={(name, format) => void controller.createCompressed(name, format)} />
</section>

<style>
  .swipe-stage {
    position: relative;
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }
  .swipe-pane {
    position: absolute;
    inset: 0;
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    background: #242220;
    will-change: transform;
  }
  .swipe-pane.current {
    z-index: 1;
  }
  .swipe-incoming-fill {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    pointer-events: none;
  }
  /* Following the finger is instant; releasing settles with a short slide. */
  .swipe-stage.swiping .swipe-pane {
    transition: none;
  }
  .swipe-stage.settling .swipe-pane {
    transition: transform 260ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  @media (prefers-reduced-motion: reduce) {
    .swipe-stage.settling .swipe-pane {
      transition: none;
    }
  }
</style>
