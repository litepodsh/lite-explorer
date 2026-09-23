<script lang="ts">
  import "../app.css";
  import { applyAppearance, listenForSystemAppearance } from "$lib/settings/appearance.js";
  import { settings } from "$lib/settings/settings.svelte.js";

  let { children } = $props();

  if (typeof window !== "undefined") settings.load();

  $effect(() => {
    if (typeof window === "undefined") return;
    applyAppearance(settings.current, document, window.matchMedia("(prefers-color-scheme: dark)").matches);
    return listenForSystemAppearance(() => settings.current, window, document);
  });
</script>

{@render children()}
