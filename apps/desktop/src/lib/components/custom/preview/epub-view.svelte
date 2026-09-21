<script lang="ts">
  import DOMPurify from "dompurify";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import ListIcon from "@lucide/svelte/icons/list";
  import { confirmExternalLink } from "./external-link.js";
  import {
    epubAssetUrl,
    openEpub,
    readEpubChapter,
    resolveInner,
    type EpubDocument,
  } from "./epub.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  // Reading surface inside a shadow root, so the book's own CSS can't leak into the app.
  const BASE_CSS = `
    :host { display: block; height: 100%; overflow: auto; background: #faf9f7; }
    .epub-body {
      max-width: 42rem; margin: 0 auto; padding: 2rem 1.5rem 4rem;
      color: #1b1a18; font-family: Georgia, "Times New Roman", serif;
      font-size: 1.05rem; line-height: 1.7;
    }
    .epub-body img, .epub-body svg, .epub-body video { max-width: 100%; height: auto; }
    .epub-body h1, .epub-body h2, .epub-body h3 { line-height: 1.25; margin: 1.6em 0 0.6em; }
    .epub-body p { margin: 0 0 1em; }
    .epub-body a { color: #1f5fa8; }
    .epub-body table { border-collapse: collapse; }
    .epub-body td, .epub-body th { border: 1px solid #d8d4cf; padding: 0.25em 0.5em; }
  `;

  let book = $state.raw<EpubDocument | null>(null);
  let index = $state(0);
  let rendered = $state("");
  let error = $state("");
  let loading = $state(false);
  let tocOpen = $state(true);
  let host = $state<HTMLElement | null>(null);
  let shadow: ShadowRoot | null = null;
  let token = 0;

  const current = $derived(book?.chapters[index] ?? null);
  const chapterPaths = $derived(new Set(book?.chapters.map((chapter) => chapter.href) ?? []));

  function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    book = null;
    index = 0;
    rendered = "";
    error = "";
    openEpub(target)
      .then((document) => {
        if (request !== token) return;
        book = document;
        if (document.chapters.length > 0) void loadChapter(0);
      })
      .catch((reason) => {
        if (request === token) error = message(reason);
      });
  });

  function ensureShadow(): ShadowRoot | null {
    if (!host) return null;
    if (!shadow) {
      shadow = host.attachShadow({ mode: "open" });
      shadow.addEventListener("click", handleClick);
    }
    return shadow;
  }

  // Renders the prepared chapter into the isolated shadow root.
  $effect(() => {
    const root = ensureShadow();
    if (root) root.innerHTML = rendered;
  });

  async function loadChapter(next: number) {
    if (!book || next < 0 || next >= book.chapters.length) return;
    const request = token;
    index = next;
    loading = true;
    try {
      const raw = await readEpubChapter(path, book.chapters[next].href);
      if (request !== token) return;
      const html = await prepare(raw, book.chapters[next].href, book);
      if (request !== token) return;
      rendered = html;
    } catch (reason) {
      if (request === token) error = message(reason);
    } finally {
      if (request === token) loading = false;
    }
  }

  async function prepare(raw: string, chapterHref: string, document: EpubDocument): Promise<string> {
    const sheets = await Promise.all(
      document.styles.map(async (href) => {
        try {
          const response = await fetch(epubAssetUrl(document.base, href));
          return response.ok ? await response.text() : "";
        } catch {
          return "";
        }
      }),
    );
    // Drop @import (remote fetches) and any style-closing sequence that could break out.
    const css = sheets.join("\n").replace(/@import[^;]+;/gi, "").replace(/<\/style/gi, "");

    const parsed = new DOMParser().parseFromString(raw, "text/html");
    parsed.querySelectorAll("script, link, base, iframe, object, embed").forEach((node) => node.remove());

    for (const element of parsed.querySelectorAll<HTMLElement>("[src], [poster]")) {
      for (const attribute of ["src", "poster"]) {
        const value = element.getAttribute(attribute);
        if (!value) continue;
        const inner = resolveInner(chapterHref, value);
        if (inner) element.setAttribute(attribute, epubAssetUrl(document.base, inner));
      }
    }

    for (const anchor of parsed.querySelectorAll<HTMLAnchorElement>("a[href]")) {
      const inner = resolveInner(chapterHref, anchor.getAttribute("href") ?? "");
      if (!inner) continue;
      const target = inner.split("#")[0];
      if (chapterPaths.has(target)) {
        anchor.dataset.chapter = target;
        anchor.setAttribute("href", "#");
      } else {
        anchor.setAttribute("href", epubAssetUrl(document.base, inner));
      }
    }

    const body = parsed.body ? parsed.body.innerHTML : raw;
    const clean = DOMPurify.sanitize(body, {
      ALLOW_UNKNOWN_PROTOCOLS: true,
      FORBID_TAGS: ["script", "style", "link", "iframe", "object", "embed"],
    });
    return `<style>${BASE_CSS}${css}</style><div class="epub-body">${clean}</div>`;
  }

  function handleClick(event: Event) {
    const anchor = (event.target as HTMLElement | null)?.closest?.("a");
    if (!anchor) return;
    event.preventDefault();
    if (!anchor.dataset.chapter) {
      confirmExternalLink(anchor.getAttribute("href") ?? "");
      return;
    }
    const href = anchor.getAttribute("data-chapter");
    const target = book?.chapters.findIndex((chapter) => chapter.href === href) ?? -1;
    if (target >= 0) void loadChapter(target);
  }

  function selectToc(href: string | null) {
    if (!book || !href) return;
    const target = href.split("#")[0];
    const next = book.chapters.findIndex((chapter) => chapter.href === target);
    if (next >= 0) void loadChapter(next);
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden" role="group" aria-label="EPUB preview">
  <div class="flex shrink-0 items-center gap-2 border-b border-[#3a3734] px-2 py-1.5 text-[11px] text-[#9c9895]">
    <button
      class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836]"
      aria-label="Toggle contents"
      aria-pressed={tocOpen}
      onclick={() => (tocOpen = !tocOpen)}><ListIcon class="size-3.5" /></button>
    <button
      class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836] disabled:opacity-40"
      aria-label="Previous chapter"
      disabled={index <= 0}
      onclick={() => loadChapter(index - 1)}><ChevronLeft class="size-3.5" /></button>
    <button
      class="grid size-6 place-items-center rounded border-0 bg-transparent text-[#e8e5e2] hover:bg-[#3b3836] disabled:opacity-40"
      aria-label="Next chapter"
      disabled={!book || index >= book.chapters.length - 1}
      onclick={() => loadChapter(index + 1)}><ChevronRight class="size-3.5" /></button>
    <span class="truncate">
      {book ? `${book.title}${current ? ` — ${current.title}` : ""}` : name}
    </span>
    {#if loading}<span class="ml-auto animate-pulse">Loading…</span>{/if}
  </div>

  <div class="flex min-h-0 flex-1">
    {#if tocOpen && book}
      <nav class="w-56 shrink-0 overflow-auto border-r border-[#3a3734] py-2" aria-label="Table of contents">
        {#if book.cover}
          <img
            src={epubAssetUrl(book.base, book.cover)}
            alt="Cover"
            class="mx-3 mb-2 w-[calc(100%-1.5rem)] rounded border border-[#3a3734] object-contain" />
        {/if}
        {#if book.toc.length === 0}
          {#each book.chapters as chapter, position (chapter.href)}
            <button
              class="block w-full truncate border-0 bg-transparent px-3 py-1 text-left text-[12px] {position === index
                ? 'text-[#e8e5e2]'
                : 'text-[#9c9895] hover:text-[#e8e5e2]'}"
              onclick={() => loadChapter(position)}>{chapter.title}</button>
          {/each}
        {:else}
          {#each book.toc as entry (entry.label + (entry.href ?? ""))}
            <button
              class="block w-full truncate border-0 bg-transparent py-1 pr-3 text-left text-[12px] text-[#9c9895] hover:text-[#e8e5e2]"
              style="padding-left: {0.75 + entry.depth * 0.75}rem"
              onclick={() => selectToc(entry.href)}>{entry.label}</button>
          {/each}
        {/if}
      </nav>
    {/if}

    <div class="relative min-w-0 flex-1">
      {#if error}
        <div class="grid h-full place-content-center gap-1 px-4 text-center">
          <p class="text-[14px] font-semibold text-[#e8e5e2]">Can’t read this book</p>
          <p class="text-[12px] break-all text-[#9c9895]">{error}</p>
        </div>
      {:else}
        <div bind:this={host} class="h-full w-full bg-[#faf9f7]"></div>
      {/if}
    </div>
  </div>
</div>
