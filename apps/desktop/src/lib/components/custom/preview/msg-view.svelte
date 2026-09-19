<script lang="ts">
  import Paperclip from "@lucide/svelte/icons/paperclip";
  import { openMsg, type MsgPreview } from "./msg.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let message = $state.raw<MsgPreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    message = null;
    error = "";
    openMsg(target)
      .then((result) => {
        if (request === token) message = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="flex h-full min-h-0 flex-col overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Outlook message preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if message}
    <header class="shrink-0 border-b border-[#3a3734] px-5 py-4">
      <h1 class="text-[17px] leading-snug font-semibold">{message.subject}</h1>
      <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12.5px]">
        {#if message.sender}
          <dt class="font-medium text-[#9c9895]">De</dt><dd class="break-words">{message.sender}</dd>
        {/if}
        {#if message.to}
          <dt class="font-medium text-[#9c9895]">Para</dt><dd class="break-words">{message.to}</dd>
        {/if}
        {#if message.cc}
          <dt class="font-medium text-[#9c9895]">CC</dt><dd class="break-words">{message.cc}</dd>
        {/if}
      </dl>
    </header>
    {#if message.attachments.length > 0}
      <div class="shrink-0 flex flex-wrap gap-1.5 border-b border-[#3a3734] px-5 py-2.5">
        {#each message.attachments as attachment}
          <span class="flex items-center gap-1.5 rounded-full border border-[#3a3734] bg-white/5 px-2.5 py-1 text-[11.5px] text-[#c0bbb5]">
            <Paperclip class="size-3.5 text-[#9c9895]" />{attachment}
          </span>
        {/each}
      </div>
    {/if}
    <div class="min-h-0 flex-1 px-5 py-4">
      <pre class="font-[inherit] text-[13.5px] leading-relaxed whitespace-pre-wrap text-[#c0bbb5]">{message.body}</pre>
    </div>
  {/if}
</div>
