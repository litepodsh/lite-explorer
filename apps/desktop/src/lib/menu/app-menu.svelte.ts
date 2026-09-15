import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { exit } from "@tauri-apps/plugin-process";
import type { AboutInfo, MenuNode, PredefinedKind } from "./menu.js";

/** The app menu as rendered by the Windows/Linux title bar. Rust owns the menu; this mirrors it. */
class AppMenuStore {
  nodes = $state<MenuNode[]>([]);
  about = $state<AboutInfo | null>(null);
  #focus: HTMLElement | null = null;

  start(): () => void {
    void this.refresh();
    const unlisten = listen("menu-changed", () => void this.refresh());
    return () => void unlisten.then((stop) => stop());
  }

  async refresh() {
    try {
      this.nodes = await invoke<MenuNode[]>("app_menu");
    } catch (error) {
      console.error("Couldn't load the app menu", error);
    }
  }

  /** Remembers the focused page element so Edit items act on it after the menu takes focus. */
  rememberFocus() {
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      !active.closest(".title-bar, [data-slot='menubar-content']")
    ) {
      this.#focus = active;
    }
  }

  async run(node: MenuNode) {
    try {
      if (node.type === "item" || node.type === "check") {
        await invoke("trigger_menu", { id: node.id });
      } else if (node.type === "predefined") {
        await this.#runPredefined(node.kind, node.info);
      }
    } catch (error) {
      console.error("Menu action failed", error);
    }
  }

  async #runPredefined(kind: PredefinedKind, info: AboutInfo | null) {
    const appWindow = getCurrentWindow();
    switch (kind) {
      case "undo":
      case "redo":
      case "cut":
      case "copy":
      case "selectAll":
        await this.#restoreFocus();
        document.execCommand(kind);
        return;
      case "paste": {
        const text = await navigator.clipboard.readText();
        await this.#restoreFocus();
        document.execCommand("insertText", false, text);
        return;
      }
      case "minimize":
        await appWindow.minimize();
        return;
      case "maximize":
        await appWindow.toggleMaximize();
        return;
      case "closeWindow":
        await appWindow.close();
        return;
      case "fullscreen":
        await appWindow.setFullscreen(!(await appWindow.isFullscreen()));
        return;
      case "quit":
        await exit(0);
        return;
      case "about":
        this.about = info;
        return;
    }
  }

  /** Waits for the menu to close and hand focus back, then refocuses the remembered element. */
  async #restoreFocus() {
    await new Promise((resolve) => requestAnimationFrame(resolve));
    this.#focus?.focus();
  }
}

export const appMenu = new AppMenuStore();
