import { normalizeToken, type KeyPlatform } from "../keyboard/keys.js";
import type { KeyboardMode } from "./settings.js";

export type SettingsKeyAction =
  | { kind: "close" }
  | { kind: "section"; index: number }
  | { kind: "move"; delta: 1 | -1 };

export function settingsKeyAction(
  token: string,
  {
    platform,
    mode,
    inNav,
    sectionCount,
  }: { platform: KeyPlatform; mode: KeyboardMode; inNav: boolean; sectionCount: number },
): SettingsKeyAction | null {
  if (token === "<Esc>" || token === normalizeToken("Mod+W", platform)) return { kind: "close" };
  for (let index = 0; index < Math.min(sectionCount, 9); index++) {
    if (token === normalizeToken(`Mod+${index + 1}`, platform)) return { kind: "section", index };
  }
  if (!inNav) return null;
  if (token === "<Down>" || (mode === "yazi" && token === "j")) return { kind: "move", delta: 1 };
  if (token === "<Up>" || (mode === "yazi" && token === "k")) return { kind: "move", delta: -1 };
  return null;
}
