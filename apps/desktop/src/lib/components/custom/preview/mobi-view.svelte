<script lang="ts">
  import { openMobi, type MobiPreview } from "./mobi.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let book = $state.raw<MobiPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    book = null;
    error = "";
    openMobi(target)
      .then((result) => {
        if (request === token) book = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[var(--app-input)] text-[var(--app-fg)]" aria-label={`Book preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[var(--app-fg-muted)]">{error}</p>
    </div>
  {:else if book}
    <article class="mx-auto max-w-2xl px-6 py-8 font-serif text-[16px] leading-[1.75]">
      <header class="mb-6 border-b border-[var(--app-border)] pb-4">
        <h1 class="text-[22px] font-semibold">{book.title}</h1>
        {#if book.truncated}<p class="mt-1 text-[12px] text-[var(--app-fg-faint)]">Vista parcial del libro</p>{/if}
      </header>
      {#each book.paragraphs as paragraph, index (index)}
        <p class="mb-3 indent-6">{paragraph}</p>
      {/each}
    </article>
  {/if}
</div>
