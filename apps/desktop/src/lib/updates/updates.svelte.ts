import { getVersion } from "@tauri-apps/api/app";
import { message } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export const RELEASES_URL = "https://github.com/litepodsh/lite-explorer/releases/latest";

const FIRST_CHECK_MS = 10_000;
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "downloading"; version: string; downloaded: number; total: number | null }
  | { state: "ready"; version: string }
  | { state: "installing"; version: string };

/**
 * Checks GitHub releases for a newer version, downloads it in the background and installs it
 * on restart. Automatic checks stay quiet; "Check for Updates…" reports every outcome.
 */
class Updates {
  status = $state<UpdateStatus>({ state: "idle" });
  /** Shows download progress; automatic checks only surface a finished download. */
  manual = $state(false);
  dismissed = $state(false);
  #update: Update | null = null;

  /** Starts periodic checks. Returns a function that stops them. */
  start(): () => void {
    const first = setTimeout(() => void this.check(), FIRST_CHECK_MS);
    const every = setInterval(() => void this.check(), CHECK_EVERY_MS);
    return () => {
      clearTimeout(first);
      clearInterval(every);
    };
  }

  async check({ manual = false }: { manual?: boolean } = {}) {
    const current = this.status;
    if (current.state === "ready") {
      this.dismissed = false;
      if (manual) await this.#reportReady(current.version);
      return;
    }
    if (current.state !== "idle") {
      if (manual) this.manual = true;
      return;
    }
    this.manual = manual;
    this.dismissed = false;
    this.status = { state: "checking" };
    let update: Update | null;
    try {
      update = await check();
    } catch (error) {
      this.status = { state: "idle" };
      if (manual) await this.#reportError("Couldn’t check for updates", error);
      return;
    }
    if (!update) {
      this.status = { state: "idle" };
      if (manual) {
        const version = await getVersion();
        await message(`Lite Explorer ${version} is the latest version.`, {
          title: "You’re up to date",
          kind: "info",
        });
      }
      return;
    }
    await this.#download(update);
  }

  async #download(update: Update) {
    const version = update.version;
    this.status = { state: "downloading", version, downloaded: 0, total: null };
    try {
      await update.download((event) => {
        const status = this.status;
        if (status.state !== "downloading") return;
        if (event.event === "Started") {
          this.status = { ...status, total: event.data.contentLength ?? null };
        } else if (event.event === "Progress") {
          this.status = { ...status, downloaded: status.downloaded + event.data.chunkLength };
        }
      });
    } catch (error) {
      this.status = { state: "idle" };
      if (this.manual) await this.#reportError(`Couldn’t download Lite Explorer ${version}`, error);
      return;
    }
    this.#update = update;
    this.status = { state: "ready", version };
  }

  /** Installs the downloaded update and restarts the app. */
  async restart() {
    const update = this.#update;
    const status = this.status;
    if (!update || status.state !== "ready") return;
    this.status = { state: "installing", version: status.version };
    try {
      await update.install();
      await relaunch();
    } catch (error) {
      this.status = status;
      // Packages the updater can't replace, like .deb and .rpm, update from the release page.
      await this.#reportError(`Couldn’t install Lite Explorer ${status.version}`, error, true);
    }
  }

  dismiss() {
    this.dismissed = true;
  }

  async #reportReady(version: string) {
    await message(`Lite Explorer ${version} is downloaded. Restart the app to finish updating.`, {
      title: "Update ready",
      kind: "info",
    });
  }

  async #reportError(title: string, error: unknown, offerDownload = false) {
    const reason = error instanceof Error ? error.message : String(error);
    await message(
      offerDownload ? `${reason}\n\nThe release page will open so you can download it.` : reason,
      { title, kind: "error" },
    );
    if (offerDownload) await openUrl(RELEASES_URL).catch(() => {});
  }
}

export const updates = new Updates();
