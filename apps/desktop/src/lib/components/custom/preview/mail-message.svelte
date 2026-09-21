<script lang="ts">
  import DOMPurify from "dompurify";
  import Paperclip from "@lucide/svelte/icons/paperclip";
  import { formatSize } from "./format.js";
  import { confirmExternalLink } from "./external-link.js";
  import type { MailPreview } from "./mail.js";

  type Props = { preview: MailPreview };
  let { preview }: Props = $props();

  const BASE_CSS = `
    :host { display: block; }
    .mail-html {
      font-family: -apple-system, "Helvetica Neue", Arial, sans-serif;
      font-size: 0.95rem; line-height: 1.6; color: #1b1a18; word-wrap: break-word;
    }
    .mail-html img { max-width: 100%; height: auto; }
    .mail-html table { max-width: 100%; }
    .mail-html a { color: #1f5fa8; }
    .mail-html blockquote {
      margin: 0.5em 0; padding-left: 0.9em; border-left: 3px solid #d8d4cf; color: #55504b;
    }
  `;

  let host = $state<HTMLElement | null>(null);
  let shadow: ShadowRoot | null = null;

  const date = $derived(
    preview.date && !Number.isNaN(Date.parse(preview.date))
      ? new Date(preview.date).toLocaleString()
      : (preview.date ?? ""),
  );

  $effect(() => {
    if (!host) return;
    if (!shadow) {
      shadow = host.attachShadow({ mode: "open" });
      shadow.addEventListener("click", handleClick);
    }
    const clean = preview.html
      ? DOMPurify.sanitize(preview.html, {
          // Allow the inline `data:` images the backend embeds; scripts stay stripped.
          ALLOWED_URI_REGEXP:
            /^(?:(?:https?|mailto|tel|data|cid|xmpp):|[^a-z]|[a-z+.-]+(?:[^a-z+.:-]|$))/i,
        })
      : "";
    shadow.innerHTML = `<style>${BASE_CSS}</style><div class="mail-html">${clean}</div>`;
  });

  function handleClick(event: Event) {
    const anchor = (event.target as HTMLElement | null)?.closest?.("a");
    if (!anchor) return;
    event.preventDefault();
    confirmExternalLink(anchor.getAttribute("href") ?? "");
  }
</script>

<div class="flex h-full min-h-0 flex-col overflow-auto bg-[#faf9f7] text-[#1b1a18]">
  <header class="shrink-0 border-b border-[#e3ded8] px-5 py-4">
    <h1 class="text-[17px] leading-snug font-semibold">{preview.subject}</h1>
    <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12.5px]">
      {#if preview.from}
        <dt class="font-medium text-[#8a857f]">De</dt>
        <dd class="min-w-0 break-words">{preview.from}</dd>
      {/if}
      {#if preview.to}
        <dt class="font-medium text-[#8a857f]">Para</dt>
        <dd class="min-w-0 break-words">{preview.to}</dd>
      {/if}
      {#if preview.cc}
        <dt class="font-medium text-[#8a857f]">CC</dt>
        <dd class="min-w-0 break-words">{preview.cc}</dd>
      {/if}
      {#if date}
        <dt class="font-medium text-[#8a857f]">Fecha</dt>
        <dd>{date}</dd>
      {/if}
    </dl>
  </header>

  {#if preview.attachments.length > 0}
    <div class="shrink-0 flex flex-wrap gap-1.5 border-b border-[#e3ded8] px-5 py-2.5">
      {#each preview.attachments as attachment}
        <span
          class="flex items-center gap-1.5 rounded-full border border-[#e3ded8] bg-white px-2.5 py-1 text-[11.5px] text-[#55504b]"
          title={`${attachment.mime} · ${formatSize(attachment.size)}`}>
          <Paperclip class="size-3.5 text-[#8a857f]" />
          <span class="max-w-48 truncate">{attachment.name}</span>
          <span class="text-[#a8a29b]">{formatSize(attachment.size)}</span>
          {#if attachment.inline}
            <span class="text-[#a8a29b]">· inline</span>
          {/if}
        </span>
      {/each}
    </div>
  {/if}

  <div class="min-h-0 flex-1 px-5 py-4">
    {#if preview.html}
      <div bind:this={host}></div>
    {:else if preview.text}
      <pre class="font-[inherit] text-[13.5px] leading-relaxed whitespace-pre-wrap text-[#2c2925]"
        >{preview.text}</pre>
    {:else}
      <p class="text-[13px] text-[#8a857f]">This message has no readable body.</p>
    {/if}
  </div>
</div>
