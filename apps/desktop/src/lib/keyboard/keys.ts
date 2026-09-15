import { keyCode } from "../menu/accelerator.js";

export type KeyPlatform = "macos" | "windows" | "linux";

export type KeyInput = Pick<
  KeyboardEvent,
  "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "isComposing"
>;

const MODIFIER_ORDER = ["Ctrl", "Alt", "Shift", "Meta"] as const;
export type Modifier = (typeof MODIFIER_ORDER)[number];

const CODE_LABELS: Record<string, string> = {
  Period: ".",
  Comma: ",",
  BracketLeft: "[",
  BracketRight: "]",
  Slash: "/",
  Backslash: "\\",
  Minus: "-",
  Equal: "=",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Space: "<Space>",
  Enter: "<Enter>",
  NumpadEnter: "<Enter>",
  Tab: "<Tab>",
  Backspace: "<Backspace>",
  Delete: "<Delete>",
  Escape: "<Esc>",
  ArrowUp: "<Up>",
  ArrowDown: "<Down>",
  ArrowLeft: "<Left>",
  ArrowRight: "<Right>",
  PageUp: "<PageUp>",
  PageDown: "<PageDown>",
  Home: "<Home>",
  End: "<End>",
};

const NAMED_KEYS: Record<string, string> = {
  " ": "<Space>",
  Enter: "<Enter>",
  Tab: "<Tab>",
  Backspace: "<Backspace>",
  Delete: "<Delete>",
  Escape: "<Esc>",
  ArrowUp: "<Up>",
  ArrowDown: "<Down>",
  ArrowLeft: "<Left>",
  ArrowRight: "<Right>",
  PageUp: "<PageUp>",
  PageDown: "<PageDown>",
  Home: "<Home>",
  End: "<End>",
};

const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "AltGraph", "Meta", "CapsLock", "Fn", "OS"]);
const FUNCTION_KEY = /^F([1-9]|1[0-9]|2[0-4])$/;

/** Label for a `KeyboardEvent.code`, or null for keys no shortcut uses. */
export function codeLabel(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (FUNCTION_KEY.test(code)) return `<${code}>`;
  return CODE_LABELS[code] ?? null;
}

export function joinToken(modifiers: Iterable<Modifier>, key: string): string {
  const present = new Set(modifiers);
  return [...MODIFIER_ORDER.filter((modifier) => present.has(modifier)), key].join("+");
}

/**
 * Token for a key press. Printable keys without Ctrl, Alt or Meta follow the layout (`G`, `?`);
 * with those modifiers the physical key is used, so shortcuts work on every layout.
 */
export function eventToken(event: KeyInput): string | null {
  if (event.isComposing || event.key === "Dead" || MODIFIER_KEYS.has(event.key)) return null;
  const modifiers: Modifier[] = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Meta");

  if (event.ctrlKey || event.altKey || event.metaKey) {
    // AltGr arrives as Ctrl+Alt on Windows and types a character.
    if (event.ctrlKey && event.altKey && !event.metaKey && event.key.length === 1) return null;
    const label = codeLabel(event.code);
    return label ? joinToken(modifiers, label) : null;
  }

  const named = NAMED_KEYS[event.key] ?? (FUNCTION_KEY.test(event.key) ? `<${event.key}>` : null);
  if (named) return joinToken(modifiers, named);
  if (event.key.length !== 1) return null;
  if (/^[\x21-\x7e]$/.test(event.key)) return event.key;
  // Non-Latin layouts: letters and digits come from the physical key.
  if (/^Key[A-Z]$/.test(event.code)) {
    const letter = event.code.slice(3);
    return event.shiftKey ? letter : letter.toLowerCase();
  }
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  return null;
}

/** Canonical token for an authored keymap key such as `Mod+Shift+P` or `Ctrl+d`. */
export function normalizeToken(authored: string, platform: KeyPlatform): string {
  const parts = authored.split("+");
  const key = parts.pop() ?? "";
  if (parts.length === 0) return key;
  const modifiers = parts.map((part): Modifier => {
    if (part === "Mod") return platform === "macos" ? "Meta" : "Ctrl";
    if (part === "Ctrl" || part === "Alt" || part === "Shift" || part === "Meta") return part;
    throw new Error(`Unknown modifier “${part}” in “${authored}”`);
  });
  const commandModifier = modifiers.some((modifier) => modifier !== "Shift");
  return joinToken(modifiers, commandModifier && key.length === 1 ? key.toUpperCase() : key);
}

/** Converts a Tauri accelerator (`CmdOrCtrl+Shift+Period`) to an authored key (`Mod+Shift+.`). */
export function acceleratorToAuthored(text: string): string | null {
  const parts = text.split("+");
  const key = parts.pop();
  if (!key) return null;
  const code = keyCode(key);
  const label = code ? codeLabel(code) : null;
  if (!label) return null;
  const modifiers: string[] = [];
  for (const part of parts) {
    switch (part.toLowerCase()) {
      case "cmdorctrl":
      case "commandorcontrol":
        modifiers.push("Mod");
        break;
      case "ctrl":
      case "control":
        modifiers.push("Ctrl");
        break;
      case "cmd":
      case "command":
      case "super":
      case "meta":
        modifiers.push("Meta");
        break;
      case "alt":
      case "option":
        modifiers.push("Alt");
        break;
      case "shift":
        modifiers.push("Shift");
        break;
      default:
        return null;
    }
  }
  return [...modifiers, label].join("+");
}

const MAC_MODIFIERS: Record<Modifier, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Meta: "⌘" };
const PC_MODIFIERS: Record<Modifier, string> = { Ctrl: "Ctrl", Alt: "Alt", Shift: "Shift", Meta: "Super" };

const SHARED_NAMES: Record<string, string> = {
  "<Up>": "↑",
  "<Down>": "↓",
  "<Left>": "←",
  "<Right>": "→",
  "<Esc>": "Esc",
  "<Space>": "Space",
  "<PageUp>": "PgUp",
  "<PageDown>": "PgDn",
  "<Home>": "Home",
  "<End>": "End",
};
const MAC_NAMES: Record<string, string> = { ...SHARED_NAMES, "<Enter>": "↩", "<Backspace>": "⌫", "<Delete>": "⌦", "<Tab>": "⇥" };
const PC_NAMES: Record<string, string> = { ...SHARED_NAMES, "<Enter>": "Enter", "<Backspace>": "Backspace", "<Delete>": "Del", "<Tab>": "Tab" };

/** Label for one token: `⇧⌘P` on macOS, `Ctrl+Shift+P` elsewhere. */
export function formatToken(token: string, platform: KeyPlatform): string {
  if (token.length === 1) return token;
  const parts = token.split("+");
  const key = parts.pop() ?? "";
  const names = platform === "macos" ? MAC_NAMES : PC_NAMES;
  const name = names[key] ?? (key.startsWith("<") && key.endsWith(">") ? key.slice(1, -1) : key);
  const modifiers = parts as Modifier[];
  if (platform === "macos") return modifiers.map((modifier) => MAC_MODIFIERS[modifier]).join("") + name;
  return [...modifiers.map((modifier) => PC_MODIFIERS[modifier]), name].join("+");
}

/** Keyboard rules for the detected OS; an undetected OS behaves like Windows. */
export function toKeyPlatform(platform: string): KeyPlatform {
  return platform === "macos" || platform === "linux" ? platform : "windows";
}
