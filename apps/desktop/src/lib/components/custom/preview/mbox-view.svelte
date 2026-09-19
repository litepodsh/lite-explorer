<script lang="ts">
  import MailMessage from "./mail-message.svelte";
  import {
    openMbox,
    readMboxMessage,
    type MailPreview,
    type MailSummary,
  } from "./mail.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let messages = $state.raw<MailSummary[]>([]);
  let selected = $state(0);
  let mail = $state.raw<MailPreview | null>(null);
  let error = $state("");
  let token = 0;

  function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    messages = [];
    selected = 0;
    mail = null;
    error = "";
    openMbox(target)
      .then((result) => {
        if (request !== token) return;
        messages = result.messages;
        void select(0);
      })
      .catch((reason) => {
        if (request === token) error = message(reason);
      });
  });

  async function select(index: number) {
    const request = token;
    selected = index;
    mail = null;
    try {
      const result = await readMboxMessage(path, index);
      if (request === token) mail = result;
    } catch (reason) {
      if (request === token) error = message(reason);
    }
  }
</script>

<div class="flex h-full min-h-0" aria-label={`Mailbox preview of ${name}`}>
  {#if error}
    <div class="grid h-full flex-1 place-content-center justify-items-center gap-1 px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else}
    <aside class="flex w-64 shrink-0 flex-col border-r border-[#3a3734] bg-[#1f1d1b]">
      <p class="shrink-0 border-b border-[#3a3734] px-3 py-2 text-[11px] text-[#9c9895]">
        {messages.length} messages
      </p>
      <div class="min-h-0 flex-1 overflow-auto">
        {#each messages as item (item.index)}
          <button
            type="button"
            class="block w-full border-b border-[#2f2c29] px-3 py-2 text-left {item.index === selected
              ? 'bg-[#3b3836]'
              : 'hover:bg-[#2a2825]'}"
            onclick={() => void select(item.index)}>
            <span class="flex items-baseline justify-between gap-2">
              <span class="min-w-0 truncate text-[12.5px] font-medium text-[#e8e5e2]">{item.subject}</span>
              {#if item.date}
                <span class="shrink-0 text-[10.5px] text-[#67635f]">{item.date.slice(5, 10)}</span>
              {/if}
            </span>
            <span class="mt-0.5 block truncate text-[11px] text-[#9c9895]">{item.from}</span>
            <span class="mt-0.5 block truncate text-[11px] text-[#67635f]">{item.snippet}</span>
          </button>
        {/each}
      </div>
    </aside>
    <div class="min-h-0 min-w-0 flex-1">
      {#if mail}
        <MailMessage preview={mail} />
      {:else}
        <div class="grid h-full place-items-center">
          <span
            class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70"
            aria-label="Loading"></span>
        </div>
      {/if}
    </div>
  {/if}
</div>
