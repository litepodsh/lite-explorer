<script lang="ts">
  type Props = {
    /** Streamable `media://` URL for the font file. */
    src: string;
    name: string;
  };
  let { src, name }: Props = $props();

  const TEXT_KEY = "preview-font-text";
  const SIZE_KEY = "preview-font-size";
  const FAMILY = "lite-explorer-preview-font";

  const DEFAULT_TEXT = "El veloz murciélago hindú comía feliz cardillo y kiwi\n0123456789 · ¡Hola, mundo!";
  const SAMPLES = [
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "abcdefghijklmnopqrstuvwxyz",
    "0123456789",
    "!#$%&()*+,-./:;<=>?@[]^_{|}~",
    "ÁÉÍÓÚÜÑáéíóúüñ ¿¡ «» €",
  ];

  let text = $state(localStorage.getItem(TEXT_KEY) ?? DEFAULT_TEXT);
  let size = $state(Number(localStorage.getItem(SIZE_KEY)) || 44);
  let weight = $state(400);
  let italic = $state(false);

  // Register the font once per previewed file; removing the style drops it again.
  $effect(() => {
    const element = document.createElement("style");
    element.textContent = `@font-face { font-family: "${FAMILY}"; src: url("${src}"); font-display: swap; }`;
    document.head.appendChild(element);
    return () => element.remove();
  });

  $effect(() => localStorage.setItem(TEXT_KEY, text));
  $effect(() => localStorage.setItem(SIZE_KEY, String(size)));

  const fontStyle = $derived(
    `font-family: "${FAMILY}", sans-serif; font-weight: ${weight}; font-style: ${italic ? "italic" : "normal"};`,
  );

  function reset() {
    text = DEFAULT_TEXT;
    size = 44;
    weight = 400;
    italic = false;
  }
</script>

<div class="flex h-full w-full flex-col overflow-hidden" role="group" aria-label="Font preview">
  <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-[#3a3734] px-3 py-2 text-[11px] text-[#9c9895]">
    <label class="flex items-center gap-1.5">
      <span>Size</span>
      <input
        type="range"
        min="10"
        max="160"
        step="1"
        bind:value={size}
        class="h-1 w-28 accent-[#e8e5e2]"
        aria-label="Font size" />
      <span class="w-10 tabular-nums text-[#e8e5e2]">{size}px</span>
    </label>
    <label class="flex items-center gap-1.5">
      <span>Weight</span>
      <select
        bind:value={weight}
        class="rounded border border-[#3a3734] bg-[#2d2a28] px-1.5 py-0.5 text-[#e8e5e2]"
        aria-label="Font weight">
        {#each [300, 400, 500, 600, 700, 800, 900] as value (value)}
          <option {value}>{value}</option>
        {/each}
      </select>
    </label>
    <button
      class="rounded border border-[#3a3734] px-2 py-0.5 {italic
        ? 'bg-[#3b3836] text-[#e8e5e2]'
        : 'bg-transparent hover:text-[#e8e5e2]'}"
      aria-pressed={italic}
      onclick={() => (italic = !italic)}>Italic</button>
    <button
      class="ml-auto rounded border border-[#3a3734] px-2 py-0.5 hover:text-[#e8e5e2]"
      onclick={reset}>Reset</button>
  </div>

  <div class="min-h-0 flex-1 overflow-auto p-4" style={fontStyle}>
    <textarea
      bind:value={text}
      spellcheck="false"
      rows="3"
      class="mb-4 w-full resize-y rounded-md border border-[#3a3734] bg-[#1a1917] p-3 leading-[1.25] text-[#e8e5e2] outline-none"
      style="{fontStyle} font-size: {size}px;"
      aria-label="Preview text"></textarea>

    <div class="space-y-1 break-words text-[#e8e5e2]" style="font-size: {Math.max(12, Math.round(size * 0.4))}px">
      {#each SAMPLES as sample (sample)}
        <p class="whitespace-pre-wrap">{sample}</p>
      {/each}
    </div>
  </div>

  <p class="shrink-0 truncate border-t border-[#3a3734] px-3 py-1 text-[11px] text-[#9c9895]">{name}</p>
</div>
