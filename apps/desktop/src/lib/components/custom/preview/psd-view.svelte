<script lang="ts">
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import Layers from "@lucide/svelte/icons/layers";
  import { openPsd, type PsdPreview } from "./psd.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let psd = $state.raw<PsdPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    psd = null;
    error = "";
    openPsd(target)
      .then((result) => {
        if (request === token) psd = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Photoshop preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if psd}
    <div class="mx-auto max-w-2xl p-4">
      <header class="mb-3 flex items-center gap-2">
        <Layers class="size-5 text-[#9c9895]" />
        <h1 class="text-[15px] font-semibold">{name}</h1>
        <span class="text-[12px] text-[#67635f]">{psd.width}×{psd.height} · {psd.colorMode} · {psd.layers.length} capas</span>
      </header>
      <ul class="rounded-lg border border-[#3a3734] bg-[#242220]">
        {#each psd.layers as layer (layer.name)}
          <li class="flex items-center gap-2 border-b border-[#3a3734] px-3 py-1.5 text-[12.5px] last:border-0">
            {#if layer.visible}
              <Eye class="size-3.5 shrink-0 text-[#9c9895]" />
            {:else}
              <EyeOff class="size-3.5 shrink-0 text-[#67635f]" />
            {/if}
            <span class="min-w-0 flex-1 truncate {layer.visible ? '' : 'text-[#67635f]'}">{layer.name}</span>
            <span class="shrink-0 rounded bg-white/5 px-1.5 py-0.5 text-[10.5px] text-[#c0bbb5]">{layer.kind}</span>
            <span class="shrink-0 text-[11px] text-[#67635f]">{layer.width}×{layer.height}</span>
          </li>
        {/each}
      </ul>
      {#if psd.truncated}<p class="mt-2 text-[11px] text-[#67635f]">Mostrando las primeras 2000 capas</p>{/if}
    </div>
  {/if}
</div>
