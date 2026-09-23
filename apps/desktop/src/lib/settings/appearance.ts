import type { Settings, Theme, ThemeMode } from "./settings.js";

type Appearance = Pick<Settings, "theme" | "themeMode" | "radius" | "elevation" | "texture">;

export function resolveTheme(theme: Theme, mode: ThemeMode, prefersDark: boolean): Theme {
  return mode === "system" ? (prefersDark ? "dark" : "light") : theme;
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
  root.style.colorScheme = theme === "light" || theme === "off-white" ? "light" : "dark";
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
