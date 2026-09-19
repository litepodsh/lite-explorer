<script lang="ts">
  import { languageForFence } from "./languages.js";
  import { ensureLanguage, loadMonaco } from "./monaco.js";

  type Props = { source: string; language: string };
  let { source, language }: Props = $props();

  let code = $state<HTMLElement | null>(null);
  const id = $derived(languageForFence(language) || "plaintext");

  $effect(() => {
    const element = code;
    const text = source;
    const lang = id;
    if (!element) return;
    element.textContent = text;
    if (lang === "plaintext") return;
    let cancelled = false;
    loadMonaco()
      .then((monaco) =>
        ensureLanguage(lang).then(() => monaco.editor.colorize(text, lang, { tabSize: 2 })),
      )
      .then((html) => {
        if (!cancelled && element.isConnected) element.innerHTML = html;
      })
      .catch(() => {
        // Stays uncolored.
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<pre class="overflow-auto bg-[#191817] px-3 py-2 font-mono text-[12.5px] leading-relaxed text-[#e8e5e2]"><code
    bind:this={code}
    class="font-[inherit]"></code></pre>
