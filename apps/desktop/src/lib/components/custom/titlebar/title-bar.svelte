<script lang="ts">
  import { onMount } from "svelte";
  import AboutDialog from "./about-dialog.svelte";
  import MenuBar from "./menu-bar.svelte";
  import WindowControls from "./window-controls.svelte";
  import { appMenu } from "$lib/menu/app-menu.svelte.js";
  import { findShortcut } from "$lib/menu/menu.js";

  let { platform }: { platform: "windows" | "linux" } = $props();
  let menuBar = $state<ReturnType<typeof MenuBar>>();

  onMount(() => {
    const stopMenu = appMenu.start();
    let altAlone = false;

    // Capture phase: a menu shortcut wins over page handlers, like a native menu accelerator.
    const onKeydown = (event: KeyboardEvent) => {
      altAlone = event.key === "Alt" && !event.ctrlKey && !event.shiftKey && !event.metaKey;
      const plain = !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
      if (event.key === "F10" && plain) {
        event.preventDefault();
        menuBar?.focusFirst();
        return;
      }
      const node = findShortcut(appMenu.nodes, event, platform);
      if (!node) return;
      event.preventDefault();
      event.stopPropagation();
      void appMenu.run(node);
    };
    const onKeyup = (event: KeyboardEvent) => {
      if (event.key === "Alt" && altAlone) {
        event.preventDefault();
        menuBar?.focusFirst();
      }
      altAlone = false;
    };

    window.addEventListener("keydown", onKeydown, { capture: true });
    window.addEventListener("keyup", onKeyup, { capture: true });
    return () => {
      stopMenu();
      window.removeEventListener("keydown", onKeydown, { capture: true });
      window.removeEventListener("keyup", onKeyup, { capture: true });
    };
  });
</script>

<header class="title-bar" data-tauri-drag-region>
  <MenuBar bind:this={menuBar} nodes={appMenu.nodes} {platform} />
  <div class="title-bar-drag" data-tauri-drag-region></div>
  <WindowControls {platform} />
</header>
<AboutDialog />
