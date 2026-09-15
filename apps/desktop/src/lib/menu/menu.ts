import {
  formatAccelerator,
  matchesAccelerator,
  parseAccelerator,
  type KeyInput,
  type Platform,
} from "./accelerator.js";

export type PredefinedKind =
  | "undo"
  | "redo"
  | "cut"
  | "copy"
  | "paste"
  | "selectAll"
  | "minimize"
  | "maximize"
  | "closeWindow"
  | "quit"
  | "fullscreen"
  | "about";

export type AboutInfo = { name: string; version: string; description: string; copyright: string };

/** Mirror of `MenuNode` in `src-tauri/src/menu.rs`. */
export type MenuNode =
  | { type: "submenu"; label: string; children: MenuNode[] }
  | { type: "item"; id: string; label: string; accelerator: string | null; enabled: boolean }
  | {
      type: "check";
      id: string;
      label: string;
      accelerator: string | null;
      enabled: boolean;
      checked: boolean;
    }
  | { type: "separator" }
  | {
      type: "predefined";
      kind: PredefinedKind;
      label: string;
      accelerator: string | null;
      info: AboutInfo | null;
    };

/** Predefined items whose shortcut the title bar dispatches. Edit shortcuts stay with the webview. */
const DISPATCHED_PREDEFINED: readonly PredefinedKind[] = ["minimize"];

function acceleratorOf(node: MenuNode): string | null {
  if (node.type === "item" || node.type === "check") return node.enabled ? node.accelerator : null;
  if (node.type === "predefined")
    return DISPATCHED_PREDEFINED.includes(node.kind) ? node.accelerator : null;
  return null;
}

export function findShortcut(
  nodes: MenuNode[],
  event: KeyInput,
  platform: Platform,
): MenuNode | null {
  for (const node of nodes) {
    if (node.type === "submenu") {
      const found = findShortcut(node.children, event, platform);
      if (found) return found;
      continue;
    }
    const text = acceleratorOf(node);
    const accelerator = text ? parseAccelerator(text, platform) : null;
    if (accelerator && matchesAccelerator(event, accelerator)) return node;
  }
  return null;
}

export function shortcutLabel(node: MenuNode, platform: Platform): string | null {
  if (node.type === "submenu" || node.type === "separator" || !node.accelerator) return null;
  const accelerator = parseAccelerator(node.accelerator, platform);
  return accelerator ? formatAccelerator(accelerator) : null;
}
