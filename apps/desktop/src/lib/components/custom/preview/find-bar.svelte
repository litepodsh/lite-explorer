<script lang="ts">
  import { onMount } from "svelte";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import XIcon from "@lucide/svelte/icons/x";
  import { findMatches } from "./find-matches.js";

  type Props = { root: HTMLElement; onClose: () => void };
  let { root, onClose }: Props = $props();

  const MAX_MATCHES = 10_000;
  const SEARCH_DELAY_MS = 80;
  const ALL_MATCHES = "preview-find";
  const CURRENT_MATCH = "preview-find-current";
  const supportsHighlights = typeof CSS !== "undefined" && "highlights" in CSS;

  type TextStart = { node: Text; start: number };

  let input: HTMLInputElement;
  let query = $state("");
  let searchedQuery = $state("");
  let ranges = $state.raw<Range[]>([]);
  let index = $state(0);
  let previousFocus: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const countLabel = $derived(ranges.length ? `${index + 1}/${ranges.length}` : searchedQuery ? "0/0" : "");

  onMount(() => {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    input.focus();
    return () => {
      clearTimeout(timer);
      clearHighlights();
    };
  });

  export function focus() {
    input.focus();
    input.select();
  }

  function indexText(): { text: string; nodes: TextStart[] } {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const nodes: TextStart[] = [];
    let text = "";
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      nodes.push({ node: node as Text, start: text.length });
      text += node.nodeValue ?? "";
    }
    return { text, nodes };
  }

  function locate(nodes: TextStart[], offset: number): [Text, number] {
    let low = 0;
    let high = nodes.length - 1;
    while (low < high) {
      const middle = (low + high + 1) >> 1;
      if (nodes[middle].start <= offset) low = middle;
      else high = middle - 1;
    }
    return [nodes[low].node, offset - nodes[low].start];
  }

  function search() {
    clearTimeout(timer);
    searchedQuery = query;
    const { text, nodes } = indexText();
    ranges =
      nodes.length === 0
        ? []
        : findMatches(text, query, MAX_MATCHES).map(([start, end]) => {
            const range = document.createRange();
            range.setStart(...locate(nodes, start));
            range.setEnd(...locate(nodes, end));
            return range;
          });
    index = 0;
    paint();
  }

  function paint() {
    const current = ranges[index];
    if (supportsHighlights) {
      CSS.highlights.set(ALL_MATCHES, new Highlight(...ranges));
      if (current) CSS.highlights.set(CURRENT_MATCH, new Highlight(current));
      else CSS.highlights.delete(CURRENT_MATCH);
    }
    current?.startContainer.parentElement?.scrollIntoView({ block: "center" });
  }

  function move(step: number) {
    if (query !== searchedQuery) {
      search();
      return;
    }
    if (!ranges.length) return;
    index = (index + step + ranges.length) % ranges.length;
    paint();
  }

  function clearHighlights() {
    if (!supportsHighlights) return;
    CSS.highlights.delete(ALL_MATCHES);
    CSS.highlights.delete(CURRENT_MATCH);
  }

  function close() {
    clearHighlights();
    onClose();
    previousFocus?.focus();
  }

  function handleInput() {
    clearTimeout(timer);
    timer = setTimeout(search, SEARCH_DELAY_MS);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      move(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }
</script>

<div
  class="absolute top-2 right-2 z-10 flex items-center gap-1 rounded-md border border-[var(--app-border)] bg-[var(--app-surface)] py-1 pr-1 pl-2 text-[12px] text-[var(--app-fg)] shadow-[var(--app-shadow)]"
  role="search">
  <input
    bind:this={input}
    bind:value={query}
    oninput={handleInput}
    onkeydown={handleKeydown}
    aria-label="Find in file"
    placeholder="Find"
    class="w-36 border-0 bg-transparent text-[12px] text-[var(--app-fg)] outline-none placeholder:text-[var(--app-fg-muted)]" />
  <span class="min-w-10 text-right text-[var(--app-fg-muted)] tabular-nums" aria-live="polite">{countLabel}</span>
  <button
    class="grid size-6 place-items-center rounded border-0 bg-transparent text-[var(--app-fg-muted)] hover:bg-[var(--app-surface-raised)] hover:text-[var(--app-fg)]"
    aria-label="Previous match"
    onclick={() => move(-1)}><ChevronUpIcon class="size-3.5" /></button>
  <button
    class="grid size-6 place-items-center rounded border-0 bg-transparent text-[var(--app-fg-muted)] hover:bg-[var(--app-surface-raised)] hover:text-[var(--app-fg)]"
    aria-label="Next match"
    onclick={() => move(1)}><ChevronDownIcon class="size-3.5" /></button>
  <button
    class="grid size-6 place-items-center rounded border-0 bg-transparent text-[var(--app-fg-muted)] hover:bg-[var(--app-surface-raised)] hover:text-[var(--app-fg)]"
    aria-label="Close find"
    onclick={close}><XIcon class="size-3.5" /></button>
</div>
