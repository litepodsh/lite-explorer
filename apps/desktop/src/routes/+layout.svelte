<script lang="ts">
  import "../app.css";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { applyAppearance, listenForSystemAppearance, windowTheme } from "#lib/settings/appearance.js";
  import { settings } from "#lib/settings/settings.svelte.js";

  let { children } = $props();

  if (typeof window !== "undefined") settings.load();

  // The native window follows the system in system mode, or the picked theme otherwise. Without
  // this the web view could keep reporting a dark system and the light side of a pair never showed.
  $effect(() => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    const theme = windowTheme(settings.current.theme, settings.current.themeMode);
    void getCurrentWindow().setTheme(theme).catch(() => {});
  });

  $effect(() => {
    if (typeof window === "undefined") return;
    applyAppearance(settings.current, document, window.matchMedia("(prefers-color-scheme: dark)").matches);
    return listenForSystemAppearance(() => settings.current, window, document);
  });
</script>

{@render children()}
