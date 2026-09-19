<script lang="ts">
  import { openFb2, type Fb2Preview } from "./fb2.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let book = $state.raw<Fb2Preview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    book = null;
    error = "";
    openFb2(target)
      .then((result) => {
        if (request === token) book = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Book preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if book}
    <article class="mx-auto max-w-2xl px-6 py-8 font-serif text-[16px] leading-[1.75]">
      <header class="mb-6 border-b border-[#3a3734] pb-4">
        <h1 class="text-[22px] font-semibold">{book.title}</h1>
        {#if book.author}<p class="mt-1 text-[14px] text-[#9c9895]">{book.author}</p>{/if}
        {#if book.annotation}
          <p class="mt-3 text-[13.5px] italic text-[#c0bbb5]">{book.annotation}</p>
        {/if}
      </header>
      {#each book.paragraphs as paragraph, index (index)}
        {#if paragraph.startsWith("# ")}
          <h2 class="mt-6 mb-2 text-[18px] font-semibold">{paragraph.slice(2)}</h2>
        {:else}
          <p class="mb-3 indent-6">{paragraph}</p>
        {/if}
      {/each}
    </article>
  {/if}
</div>
