<script lang="ts">
  import Building2 from "@lucide/svelte/icons/building-2";
  import Globe from "@lucide/svelte/icons/globe";
  import Mail from "@lucide/svelte/icons/mail";
  import MapPin from "@lucide/svelte/icons/map-pin";
  import Phone from "@lucide/svelte/icons/phone";
  import UserRound from "@lucide/svelte/icons/user-round";
  import { openVcards, type Contact } from "./contact.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let contacts = $state.raw<Contact[]>([]);
  let error = $state("");
  let token = 0;

  function initials(value: string): string {
    return value
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase() ?? "")
      .join("");
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    contacts = [];
    error = "";
    openVcards(target)
      .then((result) => {
        if (request === token) contacts = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Contacts preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else}
    <div class="mx-auto flex max-w-2xl flex-col gap-3 p-4">
      {#each contacts as contact (contact.name + contact.emails.join())}
        <article class="rounded-xl border border-[#3a3734] bg-[#242220] p-4">
          <header class="flex items-center gap-3">
            <span class="grid size-11 shrink-0 place-items-center rounded-full bg-white/5 text-[15px] font-semibold text-[#c0bbb5]">
              {#if initials(contact.name)}{initials(contact.name)}{:else}<UserRound class="size-5" />{/if}
            </span>
            <div class="min-w-0">
              <h2 class="truncate text-[15px] font-semibold">{contact.name}</h2>
              {#if contact.title || contact.organization}
                <p class="truncate text-[12.5px] text-[#9c9895]">
                  {[contact.title, contact.organization].filter(Boolean).join(" · ")}
                </p>
              {/if}
            </div>
          </header>
          <div class="mt-3 grid gap-1.5 text-[13px]">
            {#each contact.emails as email}
              <a class="flex items-center gap-2 text-[#60a5fa] hover:underline" href="mailto:{email}">
                <Mail class="size-3.5 shrink-0 text-[#67635f]" /><span class="truncate">{email}</span>
              </a>
            {/each}
            {#each contact.phones as phone}
              <span class="flex items-center gap-2">
                <Phone class="size-3.5 shrink-0 text-[#67635f]" /><span class="truncate">{phone}</span>
              </span>
            {/each}
            {#each contact.addresses as address}
              <span class="flex items-center gap-2">
                <MapPin class="size-3.5 shrink-0 text-[#67635f]" /><span class="truncate">{address}</span>
              </span>
            {/each}
            {#each contact.urls as url}
              <a class="flex items-center gap-2 text-[#60a5fa] hover:underline" href={url}>
                <Globe class="size-3.5 shrink-0 text-[#67635f]" /><span class="truncate">{url}</span>
              </a>
            {/each}
            {#if contact.organization && contact.emails.length === 0 && contact.phones.length === 0}
              <span class="flex items-center gap-2">
                <Building2 class="size-3.5 shrink-0 text-[#67635f]" /><span class="truncate">{contact.organization}</span>
              </span>
            {/if}
          </div>
          {#if contact.note}
            <p class="mt-3 border-t border-[#3a3734] pt-3 text-[12.5px] whitespace-pre-wrap text-[#c0bbb5]">{contact.note}</p>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</div>
