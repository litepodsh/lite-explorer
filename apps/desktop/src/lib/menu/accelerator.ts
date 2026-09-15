export type Platform = "macos" | "windows" | "linux";

/** A Tauri accelerator resolved for one platform. `code` is a `KeyboardEvent.code` value. */
export type Accelerator = {
  ctrl: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
  code: string;
};

export type KeyInput = Pick<KeyboardEvent, "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">;

const NAMED_CODES: Record<string, string> = {
  period: "Period",
  ".": "Period",
  comma: "Comma",
  ",": "Comma",
  "[": "BracketLeft",
  bracketleft: "BracketLeft",
  "]": "BracketRight",
  bracketright: "BracketRight",
  slash: "Slash",
  "/": "Slash",
  minus: "Minus",
  "-": "Minus",
  equal: "Equal",
  "=": "Equal",
  space: "Space",
  enter: "Enter",
  tab: "Tab",
  backspace: "Backspace",
  delete: "Delete",
  escape: "Escape",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
};

const KEY_LABELS: Record<string, string> = {
  Period: ".",
  Comma: ",",
  BracketLeft: "[",
  BracketRight: "]",
  Slash: "/",
  Minus: "-",
  Equal: "=",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

export function keyCode(key: string): string | null {
  if (/^[a-z]$/i.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(key)) return key.toUpperCase();
  return NAMED_CODES[key.toLowerCase()] ?? null;
}

/** Parses a Tauri accelerator string such as `CmdOrCtrl+Shift+Period`. */
export function parseAccelerator(text: string, platform: Platform): Accelerator | null {
  const parts = text.split("+");
  const key = parts.pop();
  if (!key) return null;
  const accelerator: Accelerator = { ctrl: false, meta: false, alt: false, shift: false, code: "" };
  for (const part of parts) {
    switch (part.toLowerCase()) {
      case "cmdorctrl":
      case "commandorcontrol":
        if (platform === "macos") accelerator.meta = true;
        else accelerator.ctrl = true;
        break;
      case "ctrl":
      case "control":
        accelerator.ctrl = true;
        break;
      case "cmd":
      case "command":
      case "super":
      case "meta":
        accelerator.meta = true;
        break;
      case "alt":
      case "option":
        accelerator.alt = true;
        break;
      case "shift":
        accelerator.shift = true;
        break;
      default:
        return null;
    }
  }
  const code = keyCode(key);
  return code ? { ...accelerator, code } : null;
}

/** Formats for the Windows/Linux title bar, e.g. `Ctrl+Shift+.`. */
export function formatAccelerator(accelerator: Accelerator): string {
  const parts: string[] = [];
  if (accelerator.ctrl) parts.push("Ctrl");
  if (accelerator.meta) parts.push("Super");
  if (accelerator.shift) parts.push("Shift");
  if (accelerator.alt) parts.push("Alt");
  const { code } = accelerator;
  if (code.startsWith("Key")) parts.push(code.slice(3));
  else if (code.startsWith("Digit")) parts.push(code.slice(5));
  else parts.push(KEY_LABELS[code] ?? code);
  return parts.join("+");
}

/** Compares by `code` so shortcuts work the same on every keyboard layout. */
export function matchesAccelerator(event: KeyInput, accelerator: Accelerator): boolean {
  return (
    event.code === accelerator.code &&
    event.ctrlKey === accelerator.ctrl &&
    event.metaKey === accelerator.meta &&
    event.altKey === accelerator.alt &&
    event.shiftKey === accelerator.shift
  );
}
