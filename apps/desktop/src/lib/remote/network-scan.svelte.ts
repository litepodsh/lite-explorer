import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  mergeDiscovered,
  scanSmbServers,
  stopNetworkScan,
  type DiscoveredServer,
} from "./network-locations.js";

/** State of the "On your network" search in the add-location dialog. */
export class NetworkScan {
  servers = $state<DiscoveredServer[]>([]);
  scanning = $state(false);
  /** A scan ran at least once since the dialog opened. */
  scanned = $state(false);
  permissionDenied = $state(false);
  error = $state("");
  #scan = 0;
  #unlisten: UnlistenFn[] = [];

  async start() {
    this.stop();
    const scan = ++this.#scan;
    this.servers = [];
    this.scanning = true;
    this.scanned = true;
    this.permissionDenied = false;
    this.error = "";
    const unlisten = await Promise.all([
      listen<DiscoveredServer>("network-server", ({ payload }) => {
        if (payload.scan === scan) this.servers = mergeDiscovered(this.servers, payload);
      }),
      listen<{ scan: number; permissionDenied: boolean }>(
        "network-scan-finished",
        ({ payload }) => {
          if (payload.scan !== scan) return;
          this.permissionDenied = payload.permissionDenied;
          this.#finish();
        },
      ),
    ]);
    if (scan !== this.#scan) {
      for (const stop of unlisten) stop();
      return;
    }
    this.#unlisten = unlisten;
    scanSmbServers(scan).catch((error: { message?: string }) => {
      if (scan !== this.#scan) return;
      this.error = error?.message || "Couldn’t search the local network.";
      this.#finish();
    });
  }

  stop() {
    if (this.scanning) void stopNetworkScan().catch(() => {});
    this.#scan++;
    this.#finish();
  }

  reset() {
    this.stop();
    this.servers = [];
    this.scanned = false;
    this.permissionDenied = false;
    this.error = "";
  }

  #finish() {
    this.scanning = false;
    for (const unlisten of this.#unlisten) unlisten();
    this.#unlisten = [];
  }
}
