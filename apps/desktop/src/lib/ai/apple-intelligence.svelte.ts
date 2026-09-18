import { invoke } from "@tauri-apps/api/core";

export type AppleIntelligenceStatus = {
  available: boolean;
  /** Why the model can't be used, or "Apple Intelligence is ready" when it can. */
  reason: string;
};

const UNAVAILABLE: AppleIntelligenceStatus = {
  available: false,
  reason: "Apple Intelligence needs a Mac with Apple silicon running macOS 26 or later.",
};

/**
 * Apple Intelligence for name suggestions.
 *
 * Everything is asked of the on-device model through two commands that report "unavailable"
 * everywhere else (Intel Macs, older macOS, Windows, Linux), so callers only ever check
 * `available` — never the platform.
 */
class AppleIntelligence {
  /** `null` until the first check finishes: the rename field must not flash a button it can't use. */
  status = $state<AppleIntelligenceStatus | null>(null);

  get available(): boolean {
    return this.status?.available === true;
  }

  /** Reads availability once; the backend caches the answer, so re-checks are cheap. */
  async load(): Promise<AppleIntelligenceStatus> {
    this.status ??= await invoke<AppleIntelligenceStatus>("apple_intelligence_status").catch(
      () => UNAVAILABLE,
    );
    return this.status;
  }

  /** Drops the cached answer, so a user who just enabled Apple Intelligence gets the button. */
  async reload(): Promise<AppleIntelligenceStatus> {
    this.status = null;
    return this.load();
  }

  /** Suggests a better name for the item at `path`. Throws with the reason when it can't. */
  async suggestName(path: string): Promise<string> {
    return await invoke<string>("suggest_name", { path });
  }
}

export const appleIntelligence = new AppleIntelligence();
