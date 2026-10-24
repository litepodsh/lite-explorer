import type { Settings, Theme, ThemeMode } from "./settings.js";

type Appearance = Pick<Settings, "theme" | "themeMode" | "radius" | "elevation" | "texture" | "reduceEffects">;

/** Light/dark pairs shown side by side in the theme picker. System mode flips within the chosen pair. */
export const THEME_PAIRS: readonly { light: Theme; dark: Theme }[] = [
  { light: "explorer-light", dark: "dark" },
  { light: "off-white", dark: "off-black" },
  { light: "light", dark: "oled" },
];

const LIGHT_THEMES: readonly Theme[] = ["explorer-light", "off-white", "light"];

export function isLightTheme(theme: Theme): boolean {
  return LIGHT_THEMES.includes(theme);
}

function pairMember(theme: Theme, prefersDark: boolean): Theme {
  const pair = THEME_PAIRS.find((entry) => entry.light === theme || entry.dark === theme);
  return pair ? (prefersDark ? pair.dark : pair.light) : prefersDark ? "dark" : "light";
}

export function resolveTheme(theme: Theme, mode: ThemeMode, prefersDark: boolean): Theme {
  return mode === "system" ? pairMember(theme, prefersDark) : theme;
}

export function applyAppearance(
  settings: Appearance,
  document: Document,
  prefersDark = false,
): Theme {
  const theme = resolveTheme(settings.theme, settings.themeMode, prefersDark);
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.dataset.radius = settings.radius;
  root.dataset.elevation = settings.elevation;
  root.dataset.texture = settings.texture;
  root.dataset.reduceEffects = settings.reduceEffects ? "true" : "";
  root.style.colorScheme = isLightTheme(theme) ? "light" : "dark";
  return theme;
}

export function listenForSystemAppearance(
  getSettings: () => Appearance,
  window: Window,
  document: Document,
): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const update = () => applyAppearance(getSettings(), document, query.matches);
  query.addEventListener("change", update);
  update();
  return () => query.removeEventListener("change", update);
}
