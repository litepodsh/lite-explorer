<script lang="ts">
  import { dev } from "$app/environment";
  import { invoke } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, tick } from "svelte";
  import KeyboardIcon from "@lucide/svelte/icons/keyboard";
  import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
  import WrenchIcon from "@lucide/svelte/icons/wrench";
  import WindowControls from "$lib/components/custom/titlebar/window-controls.svelte";
  import { ConfirmHost, DialogSwitch, confirmation } from "$lib/components/custom/dialog/index.js";
  import SegmentedControl from "$lib/settings/segmented-control.svelte";
  import { settings } from "$lib/settings/settings.svelte.js";
  import type { ChordTimeout, KeyboardMode } from "$lib/settings/settings.js";
  import { settingsKeyAction } from "$lib/settings/settings-keys.js";
  import { eventToken, toKeyPlatform } from "$lib/keyboard/keys.js";
  import { platformState } from "$lib/state/platform.svelte.js";

  settings.load();

  const sections = [
    { id: "keyboard", label: "Keyboard", icon: KeyboardIcon },
    { id: "general", label: "General", icon: SlidersHorizontalIcon },
    { id: "advanced", label: "Advanced", icon: WrenchIcon },
  ] as const;
  type SectionId = (typeof sections)[number]["id"];

  let active = $state<SectionId>("keyboard");
  let platform = $state<"macos" | "windows" | "linux" | "unknown">("unknown");
  let navButtons = $state<HTMLButtonElement[]>([]);
  const current = $derived(settings.current);

  onMount(() => {
    void invoke<string>("os_detection").then((detected) => {
      platform = detected as typeof platform;
      platformState.current = platform;
      document.documentElement.dataset.platform = detected;
    });
    return settings.listen();
  });

  async function selectSection(index: number, focus = true) {
    active = sections[index].id;
    if (!focus) return;
    await tick();
    navButtons[index]?.focus();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || confirmation.open) return;
    const token = eventToken(event);
    if (!token) return;
    const target = event.target instanceof Element ? event.target : null;
    const action = settingsKeyAction(token, {
      platform: toKeyPlatform(platform),
      mode: current.keyboardMode,
      inNav: Boolean(target?.closest("[data-settings-nav]")),
      sectionCount: sections.length,
    });
    if (!action) return;
    event.preventDefault();
    if (action.kind === "close") void getCurrentWindow().close();
    else if (action.kind === "section") void selectSection(action.index);
    else {
      const index = sections.findIndex((section) => section.id === active);
      void selectSection((index + action.delta + sections.length) % sections.length);
    }
  }

  function confirmReset() {
    confirmation.ask({
      title: "Reset all settings?",
      description: "Keyboard, General and Advanced settings go back to their defaults. Window and column sizes stay as they are.",
      confirmLabel: "Reset",
      pendingLabel: "Resetting…",
      variant: "destructive",
      onconfirm: () => settings.reset(),
    });
  }
</script>

<svelte:head><title>Settings - Lite Explorer</title></svelte:head>
<svelte:window onkeydown={handleKeydown} />

<div class="settings-window" data-platform={platform}>
  <header class="settings-titlebar" data-tauri-drag-region>
    <span class="settings-title" data-tauri-drag-region>Settings</span>
    {#if platform === "windows" || platform === "linux"}
      <WindowControls {platform} />
    {/if}
  </header>

  <div class="settings-body">
    <nav class="settings-nav" aria-label="Settings sections" data-settings-nav>
      {#each sections as section, index (section.id)}
        <button
          bind:this={navButtons[index]}
          type="button"
          aria-current={active === section.id ? "page" : undefined}
          tabindex={active === section.id ? 0 : -1}
          onclick={() => void selectSection(index, false)}>
          <section.icon class="size-4" />
          <span>{section.label}</span>
        </button>
      {/each}
    </nav>

    <main class="settings-content">
      {#if active === "keyboard"}
        <h1>Keyboard</h1>
        <div class="card"><div class="card-core">
          <SegmentedControl
            label="Keyboard mode"
            description="Yazi mode adds single keys like j, k and g g. Not recommended with a screen reader."
            options={[{ value: "standard", label: "Standard" }, { value: "yazi", label: "Yazi" }]}
            value={current.keyboardMode}
            onchange={(value) => settings.set("keyboardMode", value as KeyboardMode)} />
          <DialogSwitch
            label="Show which-key popup"
            description="Lists the keys that can follow while you type a chord."
            checked={current.showWhichKey}
            onchange={(checked) => settings.set("showWhichKey", checked)} />
          <SegmentedControl
            label="Chord timeout"
            description="Time to type the second key of a chord."
            options={[{ value: 1000, label: "1 s" }, { value: 1500, label: "1.5 s" }, { value: 3000, label: "3 s" }]}
            value={current.chordTimeoutMs}
            onchange={(value) => settings.set("chordTimeoutMs", value as ChordTimeout)} />
        </div></div>
        <div class="card"><div class="card-core reset-row">
          <div>
            <span class="reset-label">All keyboard shortcuts</span>
            <p>Opens the searchable shortcuts list in the main window.</p>
          </div>
          <button type="button" class="secondary-button" onclick={() => void emit("open-shortcuts")}>View shortcuts</button>
        </div></div>
      {:else if active === "general"}
        <h1>General</h1>
        <div class="card"><div class="card-core">
          <DialogSwitch
            label="Show hidden files"
            checked={current.showHiddenFiles}
            onchange={(checked) => settings.set("showHiddenFiles", checked)} />
          <SegmentedControl
            label="View for new tabs"
            options={[{ value: "list", label: "List" }, { value: "grid", label: "Icons" }]}
            value={current.defaultViewMode}
            onchange={(value) => settings.set("defaultViewMode", value as "list" | "grid")} />
          <DialogSwitch
            label="Open preview in new panes"
            checked={current.previewOpenByDefault}
            onchange={(checked) => settings.set("previewOpenByDefault", checked)} />
          <SegmentedControl
            label="Pane layout"
            options={[{ value: "row", label: "Side by side" }, { value: "column", label: "Stacked" }]}
            value={current.panesLayout}
            onchange={(value) => settings.set("panesLayout", value as "row" | "column")} />
        </div></div>
      {:else}
        <h1>Advanced</h1>
        <div class="card"><div class="card-core">
          <DialogSwitch
            label="Show FPS meter"
            checked={current.showFps}
            onchange={(checked) => settings.set("showFps", checked)} />
          {#if dev}
            <DialogSwitch
              label="Show prototype switcher"
              description="Development builds only."
              checked={current.prototypeSwitcher}
              onchange={(checked) => settings.set("prototypeSwitcher", checked)} />
          {/if}
        </div></div>
        <div class="card"><div class="card-core reset-row">
          <div>
            <span class="reset-label">Reset settings</span>
            <p>Restores every setting on this page to its default.</p>
          </div>
          <button type="button" class="reset-button" onclick={confirmReset}>Reset…</button>
        </div></div>
      {/if}
    </main>
  </div>
</div>
<ConfirmHost />

<style>
  :global(html),
  :global(body) {
    height: 100%;
    background: #242220;
  }
  .settings-window {
    display: flex;
    height: 100vh;
    flex-direction: column;
    color: #e8e5e2;
  }
  .settings-titlebar {
    position: relative;
    display: flex;
    height: 44px;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-bottom: 1px solid rgb(255 255 255 / 6%);
    background: rgb(43 41 39 / 88%);
    backdrop-filter: blur(20px) saturate(125%);
  }
  /* Same height as the main window toolbar, so the traffic lights at y 27 sit centered. */
  .settings-window[data-platform="macos"] .settings-titlebar {
    height: 52px;
  }
  .settings-window:not([data-platform="macos"]) .settings-titlebar {
    justify-content: space-between;
    padding-left: 14px;
  }
  .settings-title {
    color: #cfcbc8;
    font-size: 12.5px;
    font-weight: 600;
  }
  .settings-body {
    display: flex;
    min-height: 0;
    flex: 1;
  }
  .settings-nav {
    display: flex;
    width: 176px;
    flex-shrink: 0;
    flex-direction: column;
    gap: 2px;
    padding: 12px 8px;
    border-right: 1px solid rgb(255 255 255 / 6%);
  }
  .settings-nav button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: #a8a4a1;
    font-size: 13px;
    text-align: left;
    transition: background-color 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .settings-nav button:hover {
    background: rgb(255 255 255 / 4%);
  }
  .settings-nav button[aria-current="page"] {
    background: #0a9bff;
    color: #fff;
  }
  .settings-nav button:focus-visible {
    outline: 2px solid rgb(10 155 255 / 0.6);
    outline-offset: 1px;
  }
  .settings-content {
    min-width: 0;
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px 28px;
  }
  h1 {
    margin: 0 0 12px;
    color: #f2f1f0;
    font-size: 15px;
    font-weight: 600;
  }
  .card {
    margin-bottom: 12px;
    padding: 4px;
    border-radius: 14px;
    background: rgb(255 255 255 / 3%);
    box-shadow: 0 0 0 1px rgb(255 255 255 / 5%);
  }
  .card-core {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px 16px;
    border-radius: 10px;
    background: #2b2927;
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 6%),
      0 8px 24px -12px rgb(0 0 0 / 35%);
  }
  .reset-row {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }
  .reset-label {
    color: #eceae8;
    font-size: 12.5px;
    font-weight: 500;
  }
  .reset-row p {
    margin: 3px 0 0;
    color: #8f8b88;
    font-size: 11.5px;
  }
  .reset-button {
    flex-shrink: 0;
    padding: 5px 12px;
    border: 0;
    border-radius: 8px;
    background: rgb(229 72 77 / 16%);
    color: #ff8a8e;
    font-size: 12px;
    font-weight: 500;
    transition: background-color 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .reset-button:hover {
    background: rgb(229 72 77 / 24%);
  }
  .reset-button:focus-visible {
    outline: 2px solid rgb(229 72 77 / 0.6);
    outline-offset: 2px;
  }
  .secondary-button {
    flex-shrink: 0;
    padding: 5px 12px;
    border: 0;
    border-radius: 8px;
    background: #3a3734;
    color: #f2f1f0;
    font-size: 12px;
    font-weight: 500;
    transition: background-color 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .secondary-button:hover {
    background: #45413e;
  }
  .secondary-button:focus-visible {
    outline: 2px solid rgb(10 155 255 / 0.6);
    outline-offset: 2px;
  }
  @media (prefers-reduced-motion: reduce) {
    .settings-nav button,
    .reset-button {
      transition: none;
    }
  }
</style>
