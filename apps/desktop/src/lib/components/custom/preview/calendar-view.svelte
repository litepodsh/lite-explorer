<script lang="ts">
  import CalendarDays from "@lucide/svelte/icons/calendar-days";
  import CheckSquare from "@lucide/svelte/icons/check-square";
  import MapPin from "@lucide/svelte/icons/map-pin";
  import UserRound from "@lucide/svelte/icons/user-round";
  import { openCalendar, type CalendarPreview } from "./calendar.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let calendar = $state.raw<CalendarPreview | null>(null);
  let error = $state("");
  let token = 0;

  function dayOf(value: string | null): string {
    if (!value) return "";
    const match = value.match(/^(\d{4})-(\d{2})-(\d{2})/);
    return match ? `${match[3]}/${match[2]}` : value;
  }

  function timeOf(value: string | null): string {
    if (!value) return "";
    const match = value.match(/^\d{4}-\d{2}-\d{2} (\d{2}:\d{2})/);
    return match ? match[1] : "";
  }

  $effect(() => {
    const target = path;
    const request = ++token;
    calendar = null;
    error = "";
    openCalendar(target)
      .then((result) => {
        if (request === token) calendar = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[var(--app-input)] text-[var(--app-fg)]" aria-label={`Calendar preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[var(--app-fg-muted)]">{error}</p>
    </div>
  {:else if calendar}
    <div class="mx-auto max-w-2xl p-4">
      <header class="mb-3 flex items-center gap-2">
        <CalendarDays class="size-5 text-[var(--app-fg-muted)]" />
        <h1 class="text-[15px] font-semibold">{calendar.name || name}</h1>
        <span class="text-[12px] text-[var(--app-fg-faint)]">{calendar.events.length}</span>
      </header>
      <ul class="flex flex-col gap-2">
        {#each calendar.events as event, index (event.uid || index)}
          <li class="flex gap-3 rounded-lg border border-[var(--app-border)] bg-[var(--app-surface)] p-3">
            <div class="grid w-11 shrink-0 place-items-start justify-items-center pt-0.5">
              <span class="text-[15px] font-semibold leading-none">{dayOf(event.start)}</span>
              <span class="text-[11px] text-[var(--app-fg-faint)]">{event.allDay ? "todo el día" : timeOf(event.start)}</span>
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5">
                {#if event.kind === "todo"}
                  <CheckSquare class="size-3.5 text-[var(--app-fg-muted)]" />
                {/if}
                <h2 class="truncate text-[13.5px] font-medium">{event.summary}</h2>
              </div>
              {#if event.location}
                <p class="mt-0.5 flex items-center gap-1.5 text-[12px] text-[var(--app-fg-muted)]">
                  <MapPin class="size-3.5" /><span class="truncate">{event.location}</span>
                </p>
              {/if}
              {#if event.organizer}
                <p class="mt-0.5 flex items-center gap-1.5 text-[12px] text-[var(--app-fg-muted)]">
                  <UserRound class="size-3.5" /><span class="truncate">{event.organizer}</span>
                </p>
              {/if}
              {#if event.description}
                <p class="mt-1 text-[12px] whitespace-pre-wrap text-[var(--app-fg-muted)]">{event.description}</p>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</div>
