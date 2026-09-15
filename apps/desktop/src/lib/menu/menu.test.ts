import { describe, expect, test } from "bun:test";
import { findShortcut, shortcutLabel, type MenuNode } from "./menu.js";

const key = (code: string, mods: Partial<{ ctrl: boolean; shift: boolean; alt: boolean }> = {}) => ({
  code,
  ctrlKey: mods.ctrl ?? false,
  metaKey: false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
});

/** Id of an item or check node, or the kind of a predefined one. */
const nameOf = (node: MenuNode | null) =>
  node && (node.type === "item" || node.type === "check" ? node.id : node.type === "predefined" ? node.kind : node.type);

const nodes: MenuNode[] = [
  {
    type: "submenu",
    label: "File",
    children: [
      { type: "item", id: "new-tab", label: "New Tab", accelerator: "CmdOrCtrl+T", enabled: true },
      { type: "item", id: "open", label: "Open", accelerator: "CmdOrCtrl+O", enabled: false },
      { type: "separator" },
      { type: "predefined", kind: "quit", label: "Exit", accelerator: null, info: null },
    ],
  },
  {
    type: "submenu",
    label: "Edit",
    children: [{ type: "predefined", kind: "copy", label: "Copy", accelerator: "CmdOrCtrl+C", info: null }],
  },
  {
    type: "submenu",
    label: "View",
    children: [
      {
        type: "submenu",
        label: "Panes",
        children: [
          {
            type: "check",
            id: "toggle-second-pane",
            label: "Show Second Pane",
            accelerator: "CmdOrCtrl+Shift+L",
            enabled: true,
            checked: false,
          },
        ],
      },
    ],
  },
  {
    type: "submenu",
    label: "Window",
    children: [{ type: "predefined", kind: "minimize", label: "Minimize", accelerator: "CmdOrCtrl+M", info: null }],
  },
];

describe("findShortcut", () => {
  test("finds enabled items, including inside nested submenus", () => {
    expect(nameOf(findShortcut(nodes, key("KeyT", { ctrl: true }), "windows"))).toBe("new-tab");
    expect(nameOf(findShortcut(nodes, key("KeyL", { ctrl: true, shift: true }), "linux"))).toBe(
      "toggle-second-pane",
    );
  });

  test("skips disabled items", () => {
    expect(findShortcut(nodes, key("KeyO", { ctrl: true }), "windows")).toBeNull();
  });

  test("leaves edit shortcuts to the webview but dispatches window ones", () => {
    expect(findShortcut(nodes, key("KeyC", { ctrl: true }), "windows")).toBeNull();
    expect(nameOf(findShortcut(nodes, key("KeyM", { ctrl: true }), "windows"))).toBe("minimize");
  });

  test("no match returns null", () => {
    expect(findShortcut(nodes, key("KeyQ", { ctrl: true }), "windows")).toBeNull();
  });
});

describe("shortcutLabel", () => {
  test("formats accelerators and returns null without one", () => {
    const file = nodes[0] as Extract<MenuNode, { type: "submenu" }>;
    expect(shortcutLabel(file.children[0], "windows")).toBe("Ctrl+T");
    expect(shortcutLabel(file.children[3], "windows")).toBeNull();
    expect(shortcutLabel(file.children[2], "windows")).toBeNull();
  });
});
