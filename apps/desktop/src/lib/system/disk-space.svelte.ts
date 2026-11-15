import { invoke } from "@tauri-apps/api/core";
import type { DiskOverview, VolumeInfo } from "#lib/components/custom/overview/types.js";
import { volumeForPath } from "./volumes.js";

/** How long a reading stays fresh. Free space moves slowly; the sidebar and status bar share it. */
const MAX_AGE_MS = 30_000;

class DiskSpace {
  volumes = $state<VolumeInfo[]>([]);
  #fetchedAt = 0;
  #pending: Promise<void> | null = null;

  /** Reads the volumes again when the last reading is stale. Safe to call often. */
  refresh(force = false): Promise<void> {
    if (this.#pending) return this.#pending;
    if (!force && Date.now() - this.#fetchedAt < MAX_AGE_MS) return Promise.resolve();
    this.#pending = invoke<DiskOverview>("disk_overview")
      .then((overview) => {
        this.volumes = overview.volumes;
        this.#fetchedAt = Date.now();
      })
      .catch(() => {})
      .finally(() => (this.#pending = null));
    return this.#pending;
  }

  volumeFor(path: string): VolumeInfo | null {
    return volumeForPath(this.volumes, path);
  }
}

export const diskSpace = new DiskSpace();
