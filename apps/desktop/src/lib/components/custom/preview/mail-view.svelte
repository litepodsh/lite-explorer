<script lang="ts">
  import MailMessage from "./mail-message.svelte";
  import { openMail, type MailPreview } from "./mail.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let mail = $state.raw<MailPreview | null>(null);
  let error = $state("");
  let loading = $state(false);
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    mail = null;
    error = "";
    loading = true;
    openMail(target)
      .then((result) => {
        if (request !== token) return;
        mail = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      })
      .finally(() => {
        if (request === token) loading = false;
      });
  });
</script>

<div class="relative h-full min-h-0" aria-label={`Email preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center gap-1 px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if mail}
    <MailMessage preview={mail} />
  {:else if loading}
    <div class="grid h-full place-items-center">
      <span
        class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70"
        aria-label="Loading"></span>
    </div>
  {/if}
</div>
