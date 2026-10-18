import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { QuickLookTargets } from "./targets.js";

/** macOS Quick Look panel state, shared by both panes. */
class QuickLookState {
  open = $state(false);
  #listening = false;

  /** Panel close and keys pressed inside it, from the native side. */
  listen() {
    if (this.#listening) return;
    this.#listening = true;
    void listen("quick-look-closed", () => (this.open = false));
    void listen<string>("quick-look-key", ({ payload }) => {
      // Back through the normal keymap, as if pressed in the list.
      const target = document.activeElement ?? document.body;
      target.dispatchEvent(new KeyboardEvent("keydown", { key: payload, bubbles: true }));
    });
  }

  async toggle(targets: QuickLookTargets) {
    if (!this.open && targets.paths.length === 0) return;
    this.listen();
    const opening = !this.open;
    this.open = opening;
    await invoke("quick_look", targets).catch((error) => {
      this.open = false;
      console.error("quick_look failed", error);
    });
  }

  update(targets: QuickLookTargets) {
    if (!this.open) return;
    if (targets.paths.length === 0) {
      this.open = false;
      void invoke("quick_look_close").catch(() => {});
      return;
    }
    void invoke("quick_look_update", targets).catch(() => {});
  }
}

export const quickLook = new QuickLookState();
