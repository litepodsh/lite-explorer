<script lang="ts">
  import { openComic, type ComicPreview } from "./comic.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let comic = $state.raw<ComicPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    comic = null;
    error = "";
    openComic(target)
      .then((result) => {
        if (request === token) comic = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#141311]" aria-label={`Comic preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if comic}
    <div class="mx-auto flex max-w-3xl flex-col items-center gap-1 py-2">
      {#each comic.pages as page, index (page)}
        <figure class="relative w-full">
          <img
            class="block w-full bg-[#1f1d1b]"
            src="{comic.base}/{page}"
            alt={`Página ${index + 1}`}
            loading="lazy" />
          <figcaption class="pointer-events-none absolute right-2 bottom-2 rounded bg-black/60 px-1.5 py-0.5 text-[11px] text-white">
            {index + 1} / {comic.pages.length}
          </figcaption>
        </figure>
      {/each}
    </div>
  {/if}
</div>
