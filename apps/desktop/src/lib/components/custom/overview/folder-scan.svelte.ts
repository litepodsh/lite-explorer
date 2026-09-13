import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { FolderUsage, FolderUsageError, FolderUsageProgress } from "./types.js";
import { isStale, upsertEntry } from "./usage.js";

/**
 * App-wide state of the home folder scan. The walk runs on a Rust thread and keeps going when
 * you leave the overview, so the status bar and the overview read the same instance.
 */
class FolderScan {
  usage = $state<FolderUsage | null>(null);
  scanning = $state(false);
  /** Bytes walked so far in the running scan. */
  scannedBytes = $state(0);
  currentFolder = $state<string | null>(null);
  error = $state("");
  /** Bumped every time a scan finishes, so views can re-read what the scan affects. */
  completions = $state(0);

  rootName = $derived(this.usage?.root.split("/").filter(Boolean).at(-1) ?? "home folder");

  #listening: Promise<void> | undefined;

  /** Subscribes to scan events once for the life of the app. */
  listen(): Promise<void> {
    this.#listening ??= Promise.all([
      listen<FolderUsageProgress>("folder-usage-progress", ({ payload }) => {
        if (this.usage && payload.root !== this.usage.root) return;
        this.scanning = true;
        this.scannedBytes = payload.scanned_bytes;
        this.currentFolder = payload.current;
        if (this.usage && payload.entry) {
          this.usage.entries = upsertEntry(this.usage.entries, payload.entry);
        }
      }),
      listen<FolderUsage>("folder-usage-done", ({ payload }) => {
        if (this.usage && payload.root !== this.usage.root) return;
        this.usage = payload;
        this.scanning = false;
        this.currentFolder = null;
        this.completions += 1;
      }),
      listen<FolderUsageError>("folder-usage-error", ({ payload }) => {
        if (this.usage && payload.root !== this.usage.root) return;
        this.scanning = false;
        this.currentFolder = null;
        this.error = payload.message;
      }),
    ]).then(() => undefined);
    return this.#listening;
  }

  /** Shows the saved scan right away and rescans when it is missing or older than 30 minutes. */
  async load(): Promise<void> {
    await this.listen();
    const usage = await invoke<FolderUsage>("folder_usage");
    this.usage = usage;
    this.scanning = usage.scanning;
    if (!usage.scanning && isStale(usage.scanned_at, Date.now() / 1000)) await this.refresh();
  }

  async refresh(): Promise<void> {
    await this.listen();
    this.error = "";
    this.scannedBytes = 0;
    this.currentFolder = null;
    this.scanning = true;
    try {
      await invoke("scan_folder_usage");
    } catch (reason) {
      this.scanning = false;
      this.error = reason instanceof Error ? reason.message : String(reason);
    }
  }
}

export const folderScan = new FolderScan();
