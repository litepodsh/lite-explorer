<script lang="ts">
  import { openWord } from "./office.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  // Rendered inside a shadow root so the document styling can't leak into the app.
  const BASE_CSS = `
    :host { display: block; height: 100%; overflow: auto; background: #faf9f7; }
    .word-body {
      max-width: 46rem; margin: 0 auto; padding: 2.5rem 1.75rem 5rem;
      color: #1b1a18; font-family: Georgia, "Times New Roman", serif;
      font-size: 1.02rem; line-height: 1.7;
    }
    .word-body h1 { font-size: 1.7rem; line-height: 1.25; margin: 0 0 0.8em; }
    .word-body h2 { font-size: 1.35rem; margin: 1.6em 0 0.5em; }
    .word-body h3, .word-body h4, .word-body h5, .word-body h6 { margin: 1.4em 0 0.4em; }
    .word-body p { margin: 0 0 0.85em; }
    .word-body .office-item { margin: 0 0 0.2em; padding-left: 1.25em; text-indent: -1.25em; }
    .word-body img { max-width: 100%; height: auto; }
    .word-body table { border-collapse: collapse; margin: 1em 0; }
    .word-body td { border: 1px solid #cfcac4; padding: 0.35em 0.6em; vertical-align: top; }
    .word-body td p { margin: 0; }
    .word-body a { color: #1f5fa8; }
  `;

  let html = $state("");
  let error = $state("");
  let loading = $state(false);
  let host = $state<HTMLElement | null>(null);
  let shadow: ShadowRoot | null = null;
  let token = 0;

  function ensureShadow(): ShadowRoot | null {
    if (!host) return null;
    if (!shadow) shadow = host.attachShadow({ mode: "open" });
    return shadow;
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    html = "";
    error = "";
    loading = true;
    openWord(target)
      .then((document) => {
        if (request !== token) return;
        html = document.html;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      })
      .finally(() => {
        if (request === token) loading = false;
      });
  });

  $effect(() => {
    const root = ensureShadow();
    if (!root) return;
    root.innerHTML = `<style>${BASE_CSS}</style><div class="word-body">${html}</div>`;
  });
</script>

<div class="relative h-full min-h-0">
  {#if error}
    <div class="grid h-full place-content-center justify-items-center gap-1 px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else}
    <div class="h-full min-h-0" bind:this={host} aria-label={`Document preview of ${name}`}></div>
    {#if loading && !html}
      <div class="pointer-events-none absolute inset-0 grid place-items-center">
        <span
          class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70"
          aria-label="Loading"></span>
      </div>
    {/if}
  {/if}
</div>
