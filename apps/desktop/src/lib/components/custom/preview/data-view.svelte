<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { openData, type DataPreview } from "./data.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let data = $state.raw<DataPreview | null>(null);
  let error = $state("");
  let collapsed = $state<Record<string, boolean>>({});
  let token = 0;

  function toggle(id: string) {
    collapsed = { ...collapsed, [id]: !collapsed[id] };
  }

  function format(value: unknown): string {
    if (typeof value === "string") return value;
    if (value === null) return "null";
    return String(value);
  }

  function valueClass(value: unknown): string {
    if (typeof value === "string") return "text-[#a3c9a8]";
    if (typeof value === "number") return "text-[#e0af68]";
    if (typeof value === "boolean") return "text-[#7aa2f7]";
    return "text-[var(--app-fg-muted)]";
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    data = null;
    collapsed = {};
    error = "";
    openData(target)
      .then((result) => {
        if (request === token) data = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

{#snippet node(label: string, value: unknown, depth: number, id: string)}
  {@const isArray = Array.isArray(value)}
  {@const isObject = value !== null && typeof value === "object" && !isArray}
  {@const entries: [string, unknown][] = isArray
    ? (value as unknown[]).map((item, index) => [String(index), item])
    : isObject
      ? Object.entries(value as Record<string, unknown>)
      : []}
  <div>
    {#if isArray || isObject}
      <button
        type="button"
        class="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-[var(--app-surface-raised)]"
        style="padding-left: {depth * 14 + 4}px"
        onclick={() => toggle(id)}>
        {#if collapsed[id]}
          <ChevronRight class="size-3.5 shrink-0 text-[var(--app-fg-faint)]" />
        {:else}
          <ChevronDown class="size-3.5 shrink-0 text-[var(--app-fg-faint)]" />
        {/if}
        <span class="text-[var(--app-accent)]">{label}</span>
        <span class="text-[11px] text-[var(--app-fg-faint)]">{isArray ? `[${entries.length}]` : `{${entries.length}}`}</span>
      </button>
      {#if !collapsed[id]}
        {#each entries as [key, child] (key)}
          {@render node(key, child, depth + 1, `${id}/${key}`)}
        {/each}
      {/if}
    {:else}
      <div class="flex items-baseline gap-1.5 rounded px-1 py-0.5" style="padding-left: {depth * 14 + 22}px">
        <span class="shrink-0 text-[var(--app-accent)]">{label}</span>
        <span class="text-[var(--app-fg-faint)]">:</span>
        <span class="min-w-0 break-words {valueClass(value)}">{format(value)}</span>
      </div>
    {/if}
  </div>
{/snippet}

<div class="h-full min-h-0 overflow-auto bg-[var(--app-input)] font-mono text-[12.5px] leading-relaxed text-[var(--app-fg)]" aria-label={`Data preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="font-sans text-[13px] text-[var(--app-fg-muted)]">{error}</p>
    </div>
  {:else if data}
    <div class="p-3">
      <div class="mb-2 flex items-center gap-2 text-[11px] text-[var(--app-fg-muted)]">
        <span class="rounded bg-white/5 px-1.5 py-0.5 uppercase">{data.format}</span>
        {#if data.truncated}<span>truncado</span>{/if}
      </div>
      {@render node("$", data.value, 0, "$")}
    </div>
  {/if}
</div>
