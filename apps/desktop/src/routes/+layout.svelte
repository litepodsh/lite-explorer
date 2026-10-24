<script lang="ts">
  import "../app.css";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { applyAppearance, listenForSystemAppearance, windowTheme } from "#lib/settings/appearance.js";
  import { settings } from "#lib/settings/settings.svelte.js";

  let { children } = $props();

  if (typeof window !== "undefined") settings.load();

  // The native window follows the system in system mode, or the picked theme otherwise. Without
  // this the web view could keep reporting a dark system and the light side of a pair never showed.
  // A forced native theme also steers `prefers-color-scheme`, and clearing it emits no media-query
  // change, so the appearance is re-applied once the native change lands: otherwise switching from
  // a manual dark theme to system mode would keep resolving the dark side of the pair forever.
  $effect(() => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    const theme = windowTheme(settings.current.theme, settings.current.themeMode);
    let stopped = false;
    void getCurrentWindow()
      .setTheme(theme)
      .catch(() => {})
      .finally(() => {
        if (stopped) return;
        applyAppearance(
          settings.current,
          document,
          window.matchMedia("(prefers-color-scheme: dark)").matches,
        );
      });
    return () => {
      stopped = true;
    };
  });

  $effect(() => {
    if (typeof window === "undefined") return;
    applyAppearance(settings.current, document, window.matchMedia("(prefers-color-scheme: dark)").matches);
    return listenForSystemAppearance(() => settings.current, window, document);
  });
</script>

{@render children()}
