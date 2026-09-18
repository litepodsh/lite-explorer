<script lang="ts">
  import DialogButton from "$lib/components/custom/dialog/dialog-button.svelte";
  import DialogShell from "$lib/components/custom/dialog/dialog-shell.svelte";
  import {
    formatDate,
    noteLines,
    releaseFor,
    releases,
    type ChangeGroup,
    type ChangeKind,
  } from "./changelog.js";
  import { whatsNew } from "./whats-new.svelte.js";

  let {
    open = $bindable(false),
    version = null,
    notes = null,
  }: { open?: boolean; version?: string | null; notes?: string | null } = $props();

  type Section = { version: string; date: string | null; groups: ChangeGroup[] };

  let showAll = $state(false);
  let incoming = $derived(noteLines(notes));
  let pinned = $derived(releaseFor(version));
  let title = $derived(version ? `What’s New in ${version}` : "Release Notes");

  let sections = $derived.by<Section[]>(() => {
    if (!version) return releases;
    if (pinned) return showAll ? releases : [pinned];
    if (incoming.length) {
      const group: ChangeGroup = { kind: "changed", label: "Changed", items: incoming };
      return [{ version, date: null, groups: [group] }];
    }
    return releases;
  });

  $effect(() => {
    if (!open && version) whatsNew.markSeen();
  });
</script>

<DialogShell
  bind:open
  {title}
  description={version
    ? "Here’s what changed in this version."
    : "Every change that shipped in Lite Explorer, newest first."}
  width={520}>
  <div class="grid max-h-[min(46vh,340px)] gap-3.5 overflow-y-auto pr-1">
    {#each sections as section (section.version)}
      <article class="grid gap-2">
        <header class="flex items-baseline gap-2">
          <span class="text-[12.5px] font-semibold text-[#eceae8]">{section.version}</span>
          {#if section.date}
            <span class="text-[11.5px] text-[#8f8b88]">{formatDate(section.date)}</span>
          {/if}
        </header>
        {#each section.groups as group (group.kind)}
          <div class="grid grid-cols-[66px_1fr] items-start gap-2.5">
            <span
              class="py-0.5 text-[11px] font-medium tracking-wide text-[#8f8b88] uppercase kind-{group
                .kind as ChangeKind}">{group.label}</span>
            <ul class="m-0 grid list-none gap-1 p-0">
              {#each group.items as item (item)}
                <li
                  class="relative pl-3 text-[12.5px] leading-relaxed text-[#cfccc9] before:absolute before:top-[7px] before:left-[2px] before:size-1 before:rounded-full before:bg-[#6b6764] before:content-['']">
                  {item}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </article>
    {/each}
  </div>
  {#snippet footer()}
    {#if version && !showAll}
      <DialogButton onclick={() => (showAll = true)}>All versions</DialogButton>
    {/if}
    <DialogButton variant="primary" onclick={() => (open = false)}>Done</DialogButton>
  {/snippet}
</DialogShell>

<style>
  .kind-added {
    color: #6fbf95;
  }
  .kind-fixed {
    color: #d8a35a;
  }
  .kind-changed {
    color: #7aa9d8;
  }
  .kind-security,
  .kind-removed {
    color: #d8736a;
  }
</style>
