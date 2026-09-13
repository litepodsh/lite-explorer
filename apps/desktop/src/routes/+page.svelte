<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
  import ArrowRightIcon from "@lucide/svelte/icons/arrow-right";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import Grid2X2Icon from "@lucide/svelte/icons/grid-2x2";
  import ListIcon from "@lucide/svelte/icons/list";
  import PanelLeftIcon from "@lucide/svelte/icons/panel-left";
  import SearchIcon from "@lucide/svelte/icons/search";
  import AppSidebar from "$lib/components/custom/sidebar/app-sidebar.svelte";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";

  let selected = $state("Recents");
  let sidebarFloating = $state(false);
  let sidebarOpen = $state(true);
  let sidebarResizing = $state(false);
  let sidebarWidth = $state(192);
  const sidebarDefaultWidth = 192;
  const sidebarMinimumWidth = 192;
  type ViewTransitionDocument = Document & {
    startViewTransition?: (update: () => void) => unknown;
  };

  onMount(() => {
    sidebarFloating = localStorage.getItem("sidebar-floating") === "true";
    const savedWidth = Number(localStorage.getItem("sidebar-width"));
    if (savedWidth >= sidebarMinimumWidth) sidebarWidth = savedWidth;
    void invoke("set_sidebar_floating", { floating: sidebarFloating });
    const unlisten = listen<boolean>("sidebar-floating", ({ payload }) => {
      sidebarFloating = payload;
      localStorage.setItem("sidebar-floating", String(payload));
    });
    return () => void unlisten.then((stop) => stop());
  });

  function resizeSidebar(width: number) {
    sidebarWidth = Math.max(
      sidebarMinimumWidth,
      Math.min(width, window.innerWidth - sidebarMinimumWidth),
    );
    sidebarOpen = true;
    localStorage.setItem("sidebar-width", String(sidebarWidth));
  }

  function toggleSidebar() {
    withSidebarViewTransition(() => {
      if (sidebarOpen) {
        sidebarOpen = false;
        return;
      }

      sidebarWidth = sidebarDefaultWidth;
      sidebarOpen = true;
      localStorage.setItem("sidebar-width", String(sidebarWidth));
    });
  }

  function withSidebarViewTransition(update: () => void) {
    const documentWithViewTransition = document as ViewTransitionDocument;
    if (documentWithViewTransition.startViewTransition) {
      documentWithViewTransition.startViewTransition(update);
      return;
    }

    update();
  }
</script>

<svelte:head><title>{selected} - Lite Explorer</title></svelte:head>

<Sidebar.Provider
  bind:open={sidebarOpen}
  style={`--sidebar-width: ${sidebarWidth}px;`}
  class={`finder-window${sidebarFloating ? " sidebar-floating" : ""}${sidebarResizing ? " sidebar-resizing" : ""}${sidebarOpen ? "" : " sidebar-collapsed"}`}>
  <AppSidebar
    bind:selected
    open={sidebarOpen}
    width={sidebarWidth}
    onResize={resizeSidebar}
    onResizeStart={() => (sidebarResizing = true)}
    onResizeEnd={() => (sidebarResizing = false)}
    onToggle={toggleSidebar}
    variant={sidebarFloating ? "floating" : "sidebar"} />
  <button
    class="sidebar-trigger"
    class:sidebar-trigger-open={sidebarOpen}
    aria-label="Toggle sidebar"
    title="Toggle sidebar"
    onclick={toggleSidebar}><PanelLeftIcon /></button>
  <Sidebar.Inset class="finder-content">
    <header class="finder-toolbar" data-tauri-drag-region>
      <div class="toolbar-controls">
        <button aria-label="Back"><ArrowLeftIcon /></button>
        <button aria-label="Forward" disabled><ArrowRightIcon /></button>
      </div>
      <button class="location-title" aria-label="Current location"
        >{selected}<ChevronDownIcon /></button>
      <div class="toolbar-actions">
        <button aria-label="List view"><ListIcon /></button>
        <button aria-label="Icon view"><Grid2X2Icon /></button>
        <label class="finder-search"
          ><SearchIcon /><input aria-label="Search" placeholder="Search" /></label>
      </div>
    </header>
    <section class="finder-empty" aria-live="polite">
      <FolderIcon />
      <h1>{selected}</h1>
      <p>Items in this location will appear here.</p>
    </section>
  </Sidebar.Inset>
</Sidebar.Provider>
