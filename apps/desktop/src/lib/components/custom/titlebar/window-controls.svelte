<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { invoke } from "@tauri-apps/api/core";
  import { settings } from "$lib/settings/settings.svelte.js";
  import { onMount } from "svelte";

  let { platform }: { platform: "windows" | "linux" } = $props();
  const appWindow = getCurrentWindow();
  let maximized = $state(false);
  let hyprland = $state(false);
  const mode = $derived(settings.current.windowControls);
  const revealOnHover = $derived(mode === "hover" || (mode === "automatic" && platform === "linux" && hyprland));

  onMount(() => {
    void invoke<boolean>("is_hyprland")
      .then((detected) => (hyprland = detected))
      .catch((error) => console.warn("Couldn't detect the desktop session", error));
  });

  onMount(() => {
    const sync = () =>
      void appWindow
        .isMaximized()
        .then((value) => (maximized = value))
        .catch((error) => console.error("Couldn't read the window state", error));
    sync();
    const unlisten = appWindow.onResized(sync);
    return () => void unlisten.then((stop) => stop());
  });

  function run(action: () => Promise<void>, name: string) {
    action().catch((error) => console.error(`Couldn't ${name} the window`, error));
  }
</script>

{#if mode !== "hidden"}
  <div class="window-controls" class:reveal-on-hover={revealOnHover} data-platform={platform}>
    <button aria-label="Minimize" title="Minimize" onclick={() => run(() => appWindow.minimize(), "minimize")}>
      {#if platform === "windows"}
        <svg viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1" /></svg>
      {:else}
        <svg viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" stroke-width="1.6" /></svg>
      {/if}
    </button>
    <button
      aria-label={maximized ? "Restore" : "Maximize"}
      title={maximized ? "Restore" : "Maximize"}
      onclick={() => run(() => appWindow.toggleMaximize(), "maximize")}>
      {#if platform === "windows" && maximized}
        <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
          <rect x="0.5" y="2.5" width="7" height="7" rx="1" />
          <path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
        </svg>
      {:else if platform === "windows"}
        <svg viewBox="0 0 10 10"
          ><rect x="0.5" y="0.5" width="9" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="1" /></svg>
      {:else}
        <svg viewBox="0 0 10 10"
          ><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1.4" /></svg>
      {/if}
    </button>
    <button class="close" aria-label="Close" title="Close" onclick={() => run(() => appWindow.close(), "close")}>
      {#if platform === "windows"}
        <svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" /></svg>
      {:else}
        <svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.6" /></svg>
      {/if}
    </button>
  </div>
{/if}

<style>
  /* Opacity preserves the hover target and keyboard access without shifting the toolbar. */
  @media (hover: hover) {
    .reveal-on-hover:not(:hover):not(:focus-within) button {
      opacity: 0;
    }
  }
  button:focus-visible {
    outline: 2px solid #0a9bff;
    outline-offset: -2px;
  }
</style>
