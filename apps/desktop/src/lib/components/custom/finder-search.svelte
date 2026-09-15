<script lang="ts">
  import SearchIcon from "@lucide/svelte/icons/search";
  import XIcon from "@lucide/svelte/icons/x";
  import type { FilePaneController } from "$lib/file-pane/controller.svelte.js";
  let { controller }: { controller: FilePaneController } = $props();
  let input: HTMLInputElement;
  let focused = $state(false);
  const expanded = $derived(focused || !!controller.searchQuery);
  export function focus() { input?.focus(); input?.select(); }
  function keydown(event: KeyboardEvent) { if (event.key === "Escape") { event.preventDefault(); controller.clearSearch(); input.blur(); } }
  // Keep focus in the input so clicking a control does not collapse the search before the click lands.
  function keepFocus(event: MouseEvent) { event.preventDefault(); }
</script>

<div class:expanded class="finder-search" role="search">
  <SearchIcon class="finder-search-icon" />
  <input bind:this={input} value={controller.searchQuery} onfocus={() => (focused = true)} onblur={() => (focused = false)} oninput={(e) => controller.setSearchQuery(e.currentTarget.value)} onkeydown={keydown} aria-label="Search this folder" placeholder={controller.searchMode === "content" ? "Search file contents" : "Search"} spellcheck="false" autocomplete="off" />
  {#if controller.searchQuery}<button type="button" class="finder-search-clear" aria-label="Clear search" onmousedown={keepFocus} onclick={() => controller.clearSearch()}><XIcon /></button>{/if}
  {#if expanded}
    <div class="finder-search-modes" class:content={controller.searchMode === "content"} role="group" aria-label="Search mode">
      <span class="finder-search-indicator" aria-hidden="true"></span>
      <button type="button" class:active={controller.searchMode === "fuzzy"} aria-pressed={controller.searchMode === "fuzzy"} onmousedown={keepFocus} onclick={() => controller.setSearchMode("fuzzy")}>Fuzzy</button>
      <button type="button" class:active={controller.searchMode === "content"} aria-pressed={controller.searchMode === "content"} onmousedown={keepFocus} onclick={() => controller.setSearchMode("content")}>Content</button>
    </div>
  {/if}
</div>
