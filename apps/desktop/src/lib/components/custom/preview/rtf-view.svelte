<script lang="ts">
  import { openRtf } from "./rtf.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  // Rendered inside a shadow root so RTF styling can't leak into the app chrome.
  const BASE_CSS = `
    :host { display: block; height: 100%; overflow: auto; background: #faf9f7; }
    .rtf-body {
      max-width: 42rem; margin: 0 auto; padding: 2rem 1.5rem 4rem;
      color: #1b1a18; font-family: Georgia, "Times New Roman", serif;
      font-size: 1.05rem; line-height: 1.7; white-space: pre-wrap;
    }
    .rtf-body p { margin: 0 0 0.85em; }
    .rtf-body br { content: ""; }
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
    openRtf(target)
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
    root.innerHTML = `<style>${BASE_CSS}</style><div class="rtf-body">${html}</div>`;
  });
</script>

<div class="relative h-full min-h-0">
  {#if error}
    <div class="grid h-full place-content-center justify-items-center gap-1 px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else}
    <div class="h-full min-h-0" bind:this={host} aria-label={`Rich text preview of ${name}`}></div>
    {#if loading && !html}
      <div class="pointer-events-none absolute inset-0 grid place-items-center">
        <span
          class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70"
          aria-label="Loading"></span>
      </div>
    {/if}
  {/if}
</div>
