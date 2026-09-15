import { appliesTo, type Binding, type Category, type Mode } from "./keymap.js";
import { joinToken, normalizeToken, type KeyPlatform, type Modifier } from "./keys.js";
import type { Scope } from "./scope.js";

export type ShortcutRow = {
  id: string;
  desc: string;
  category: Category;
  scope: Scope;
  mode: Mode;
  command?: string;
  source: "app" | "menu" | "component";
  /** Each way to press the shortcut, as canonical tokens. */
  sequences: string[][];
};

export const CATEGORY_ORDER: Category[] = ["Navigation", "Selection", "Files", "Search", "Sort", "Tabs & Panes", "Preview", "App"];

/** One row per shortcut; several keys for the same command share a row. */
export function shortcutRows(bindings: Binding[], platform: KeyPlatform): ShortcutRow[] {
  const rows = new Map<string, ShortcutRow>();
  for (const binding of bindings) {
    if (!appliesTo(binding, platform)) continue;
    const id = `${binding.mode}|${binding.scope}|${binding.command ?? binding.menuId ?? ""}|${binding.desc}`;
    const sequence = binding.keys.map((key) => normalizeToken(key, platform));
    const row = rows.get(id);
    if (row) row.sequences.push(sequence);
    else
      rows.set(id, {
        id,
        desc: binding.desc,
        category: binding.category,
        scope: binding.scope,
        mode: binding.mode,
        command: binding.command,
        source: binding.source ?? "app",
        sequences: [sequence],
      });
  }
  return [...rows.values()];
}

const MODIFIER_ALIASES: Record<string, Modifier | "Mod"> = {
  ctrl: "Ctrl",
  control: "Ctrl",
  "⌃": "Ctrl",
  alt: "Alt",
  opt: "Alt",
  option: "Alt",
  "⌥": "Alt",
  shift: "Shift",
  "⇧": "Shift",
  cmd: "Mod",
  command: "Mod",
  mod: "Mod",
  "⌘": "Mod",
  meta: "Meta",
  super: "Meta",
  win: "Meta",
};

const KEY_ALIASES: Record<string, string> = {
  space: "<Space>",
  enter: "<Enter>",
  return: "<Enter>",
  "↩": "<Enter>",
  esc: "<Esc>",
  escape: "<Esc>",
  tab: "<Tab>",
  "⇥": "<Tab>",
  up: "<Up>",
  "↑": "<Up>",
  down: "<Down>",
  "↓": "<Down>",
  left: "<Left>",
  "←": "<Left>",
  right: "<Right>",
  "→": "<Right>",
  delete: "<Delete>",
  del: "<Delete>",
  "⌦": "<Delete>",
  backspace: "<Backspace>",
  "⌫": "<Backspace>",
  pageup: "<PageUp>",
  pgup: "<PageUp>",
  pagedown: "<PageDown>",
  pgdn: "<PageDown>",
  home: "<Home>",
  end: "<End>",
};

const VIM_MODIFIERS: Record<string, string> = { c: "ctrl", a: "alt", s: "shift" };

/** Turns what people type (`g g`, `ctrl+d`, `<C-d>`, `⇧⌘P`) into the text of a canonical key. */
export function normalizeQuery(query: string, platform: KeyPlatform): string {
  let text = query.trim();
  const vim = /^<([cas])-(.+)>$/i.exec(text);
  if (vim) text = `${VIM_MODIFIERS[vim[1].toLowerCase()]}+${vim[2]}`;
  text = text.replace(/[⌃⌥⇧⌘]/g, (glyph) => ` ${glyph} `);
  const parts = text.split(/[\s+]+/).filter(Boolean);
  const modifiers = new Set<Modifier>();
  const keys: string[] = [];
  for (const part of parts) {
    const alias = MODIFIER_ALIASES[part.toLowerCase()];
    if (alias && parts.length > 1) {
      modifiers.add(alias === "Mod" ? (platform === "macos" ? "Meta" : "Ctrl") : alias);
      continue;
    }
    const lower = part.toLowerCase();
    keys.push(KEY_ALIASES[lower] ?? (/^f([1-9]|1[0-9]|2[0-4])$/i.test(part) ? `<F${part.slice(1)}>` : part));
  }
  if (keys.length === 0) return parts.join("");
  const key = keys.join("");
  if (modifiers.size === 0) return key;
  if (modifiers.size === 1 && modifiers.has("Shift") && key.length === 1) return key.toUpperCase();
  const commandModifier = [...modifiers].some((modifier) => modifier !== "Shift");
  return joinToken(modifiers, commandModifier && key.length === 1 ? key.toUpperCase() : key);
}

/**
 * Rows matching `query`, ranked like yazi's help filter: exact key, key prefix, key contains,
 * description contains; then `fuzzy` matches on the rest.
 */
export function filterShortcuts(
  rows: ShortcutRow[],
  query: string,
  platform: KeyPlatform,
  fuzzy: (rows: ShortcutRow[], query: string) => ShortcutRow[] = () => [],
): ShortcutRow[] {
  const raw = query.trim();
  if (!raw) return rows;
  const key = normalizeQuery(raw, platform);
  const lowerKey = key.toLowerCase();
  const lowerRaw = raw.toLowerCase();
  const tiers: ShortcutRow[][] = [[], [], [], []];
  for (const row of rows) {
    const texts = row.sequences.map((sequence) => sequence.join(""));
    if (texts.some((text) => text === key)) tiers[0].push(row);
    else if (texts.some((text) => text.toLowerCase().startsWith(lowerKey))) tiers[1].push(row);
    else if (texts.some((text) => text.toLowerCase().includes(lowerKey))) tiers[2].push(row);
    else if (row.desc.toLowerCase().includes(lowerRaw)) tiers[3].push(row);
  }
  const matched = tiers.flat();
  const seen = new Set(matched.map((row) => row.id));
  return [...matched, ...fuzzy(rows.filter((row) => !seen.has(row.id)), raw)];
}

/** Highlight range of `query` inside a description, for `highlightMatches`. */
export function descMatch(desc: string, query: string): [number, number][] | undefined {
  const needle = query.trim().toLowerCase();
  if (!needle) return undefined;
  const start = desc.toLowerCase().indexOf(needle);
  return start < 0 ? undefined : [[start, start + needle.length - 1]];
}
