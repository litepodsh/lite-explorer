<script lang="ts">
  import { openSubtitle, type SubtitlePreview } from "./subtitle.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let subtitle = $state.raw<SubtitlePreview | null>(null);
  let error = $state("");
  let token = 0;

  function short(value: string): string {
    const match = value.match(/(\d{2}:\d{2}:\d{2})[.,](\d{1,3})/);
    return match ? `${match[1]}.${match[2].padEnd(3, "0")}` : value;
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    subtitle = null;
    error = "";
    openSubtitle(target)
      .then((result) => {
        if (request === token) subtitle = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Subtitles preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if subtitle}
    <div class="mx-auto max-w-2xl p-4">
      <p class="mb-2 text-[11.5px] text-[#9c9895]">{subtitle.format.toUpperCase()} · {subtitle.cues.length} cues</p>
      <ol class="flex flex-col">
        {#each subtitle.cues as cue (cue.index)}
          <li class="flex gap-3 border-b border-[#3a3734] py-2 last:border-0">
            <span class="w-24 shrink-0 pt-0.5 font-mono text-[11px] text-[#67635f]">{short(cue.start)}</span>
            <span class="min-w-0 flex-1 whitespace-pre-wrap text-[13.5px]">{cue.text}</span>
          </li>
        {/each}
      </ol>
    </div>
  {/if}
</div>
