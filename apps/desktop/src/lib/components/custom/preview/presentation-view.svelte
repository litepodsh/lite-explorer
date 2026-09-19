<script lang="ts">
  import { openPresentation, type Presentation } from "./office.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  // Slides render inside a shadow root; each is a fixed-ratio card in a scroll column.
  const BASE_CSS = `
    :host { display: block; height: 100%; overflow: auto; background: #1b1a18; }
    .deck { display: flex; flex-direction: column; gap: 1.25rem; padding: 1.5rem; align-items: center; }
    .slide {
      position: relative; width: min(100%, 56rem); aspect-ratio: 16 / 9;
      background: #faf9f7; color: #1b1a18; border-radius: 0.5rem;
      box-shadow: 0 18px 40px -22px rgb(0 0 0 / 0.9); overflow: hidden;
      padding: 2.25rem 2.75rem; display: flex; flex-direction: column; gap: 0.75rem;
      font-family: "Helvetica Neue", Arial, sans-serif;
    }
    .slide h2 { font-size: 1.55rem; line-height: 1.25; margin: 0 0 0.35rem; }
    .slide p { font-size: 1.02rem; line-height: 1.5; margin: 0; }
    .slide img { max-width: 100%; max-height: 60%; object-fit: contain; align-self: center; }
    .slide .index {
      position: absolute; right: 0.75rem; bottom: 0.6rem;
      font-size: 0.7rem; color: #9c9895;
    }
  `;

  let deck = $state.raw<Presentation | null>(null);
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
    deck = null;
    error = "";
    loading = true;
    openPresentation(target)
      .then((result) => {
        if (request !== token) return;
        deck = result;
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
    if (!deck) {
      root.innerHTML = `<style>${BASE_CSS}</style>`;
      return;
    }
    const current = deck;
    const slides = current.slides
      .map(
        (slide, index) =>
          `<section class="slide"><h2>${slide.title}</h2>${slide.html}<span class="index">${
            index + 1
          } / ${current.slides.length}</span></section>`,
      )
      .join("");
    root.innerHTML = `<style>${BASE_CSS}</style><div class="deck">${slides}</div>`;
  });
</script>

<div class="relative h-full min-h-0">
  {#if error}
    <div class="grid h-full place-content-center justify-items-center gap-1 px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else}
    <div class="h-full min-h-0" bind:this={host} aria-label={`Presentation preview of ${name}`}></div>
    {#if loading && !deck}
      <div class="pointer-events-none absolute inset-0 grid place-items-center">
        <span
          class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70"
          aria-label="Loading"></span>
      </div>
    {/if}
  {/if}
</div>
