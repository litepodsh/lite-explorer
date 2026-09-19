<script lang="ts">
  import DOMPurify from "dompurify";
  import MarkdownView from "./markdown-view.svelte";
  import NotebookCode from "./notebook-code.svelte";
  import { openNotebook, type NotebookPreview } from "./notebook.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let notebook = $state.raw<NotebookPreview | null>(null);
  let error = $state("");
  let token = 0;

  function sanitized(html: string): string {
    return DOMPurify.sanitize(html, { ALLOWED_URI_REGEXP: /^(?:(?:https?|data):|[^a-z]|[a-z+.-]+(?:[^a-z+.:-]|$))/i });
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    notebook = null;
    error = "";
    openNotebook(target)
      .then((result) => {
        if (request === token) notebook = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Notebook preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if notebook}
    <div class="mx-auto max-w-3xl px-4 py-4">
      <div class="mb-3 flex items-center gap-2 text-[11.5px] text-[#9c9895]">
        {#if notebook.kernel}<span class="rounded bg-white/5 px-1.5 py-0.5">{notebook.kernel}</span>{/if}
        {#if notebook.language}<span>{notebook.language}</span>{/if}
        {#if notebook.truncated}<span>· truncado</span>{/if}
      </div>
      {#each notebook.cells as cell, index (index)}
        <section class="mb-3">
          {#if cell.kind === "markdown"}
            <div class="rounded-lg border border-[#3a3734] bg-[#242220]">
              <MarkdownView source={cell.source} />
            </div>
          {:else if cell.kind === "code"}
            <div class="overflow-hidden rounded-lg border border-[#3a3734]">
              <div class="flex items-center justify-between bg-[#242220] px-3 py-1 text-[11px] text-[#9c9895]">
                <span>{notebook.language || "code"}</span>
                <span>In [{index + 1}]</span>
              </div>
              <NotebookCode source={cell.source} language={notebook.language} />
              {#each cell.outputs as output}
                {#if output.kind === "image"}
                  <div class="border-t border-[#3a3734] bg-[#242220] px-3 py-2">
                    <img class="max-w-full" src={output.text} alt="Salida" />
                  </div>
                {:else if output.kind === "html"}
                  <div class="notebook-output border-t border-[#3a3734] bg-[#242220] px-3 py-2 text-[#e8e5e2]">
                    {@html sanitized(output.text)}
                  </div>
                {:else}
                  <pre class="overflow-auto border-t border-[#3a3734] px-3 py-2 font-mono text-[12px] leading-relaxed whitespace-pre-wrap {output.kind ===
                  'error'
                    ? 'bg-[#3a2020] text-[#f7768e]'
                    : 'bg-[#242220] text-[#c0bbb5]'}">{output.text}</pre>
                {/if}
              {/each}
            </div>
          {:else}
            <pre class="overflow-auto rounded-lg border border-[#3a3734] bg-[#191817] px-3 py-2 font-mono text-[12.5px] text-[#e8e5e2]">{cell.source}</pre>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .notebook-output :global(img) {
    max-width: 100%;
  }
  .notebook-output :global(a) {
    color: #60a5fa;
  }
  .notebook-output :global(table) {
    border-collapse: collapse;
  }
  .notebook-output :global(td),
  .notebook-output :global(th) {
    border: 1px solid #3a3734;
    padding: 2px 6px;
  }
</style>
