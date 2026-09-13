<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { languageForFence } from "./languages.js";
  import { renderMarkdown } from "./markdown.js";
  import { ensureLanguage, loadMonaco, type MonacoApi } from "./monaco.js";

  type Props = { source: string; onError?: (error: unknown) => void };
  let { source, onError }: Props = $props();

  let container: HTMLElement;

  $effect(() => {
    let fragment: DocumentFragment;
    try {
      fragment = renderMarkdown(source);
    } catch (error) {
      onError?.(error);
      return;
    }
    container.replaceChildren(fragment);
    container.scrollTop = 0;

    let cancelled = false;
    loadMonaco()
      .then((monaco) => {
        if (!cancelled) colorizeCodeBlocks(monaco);
      })
      .catch(() => {
        // Code blocks stay uncolored.
      });
    return () => {
      cancelled = true;
    };
  });

  function colorizeCodeBlocks(monaco: MonacoApi) {
    for (const code of container.querySelectorAll<HTMLElement>("pre > code[class*='language-']")) {
      const fence = [...code.classList].find((name) => name.startsWith("language-"))?.slice("language-".length) ?? "";
      const language = languageForFence(fence);
      if (language === "plaintext") continue;
      const text = code.textContent ?? "";
      ensureLanguage(language)
        .then(() => monaco.editor.colorize(text, language, { tabSize: 2 }))
        .then((html) => {
          if (code.isConnected) code.innerHTML = html;
        })
        .catch(() => {
          // Block stays uncolored.
        });
    }
  }

  function handleClick(event: MouseEvent) {
    const anchor = (event.target as Element | null)?.closest("a");
    if (!anchor) return;
    event.preventDefault();
    const href = anchor.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) void openUrl(href);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<article bind:this={container} class="markdown-body h-full overflow-auto" onclick={handleClick}></article>
