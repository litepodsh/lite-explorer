<script lang="ts">
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import XIcon from "@lucide/svelte/icons/x";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import type { JobsStore } from "$lib/transfers/jobs.svelte.js";
  import { isRunning, type Job, type JobFilter, type JobKind } from "$lib/transfers/jobs.js";

  let {
    jobs,
    open = true,
    onToggle = () => {},
  }: {
    jobs: JobsStore;
    open?: boolean;
    onToggle?: () => void;
  } = $props();

  let filter = $state<JobFilter>("all");

  const filters: { id: JobFilter; label: string }[] = [
    { id: "all", label: "All" },
    { id: "active", label: "Active" },
    { id: "done", label: "Done" },
    { id: "failed", label: "Failed" },
  ];

  const kindIcon: Record<JobKind, string> = {
    upload: "M12 19V5M6 11l6-6 6 6",
    download: "M12 5v14M6 13l6 6 6-6",
    copy: "M9 9h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2ZM5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1",
    move: "M12 19V5M6 11l6-6 6 6",
    delete: "M4 7h16M9 7V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2M6 7l1 13a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-13",
    send: "M12 19V5M6 11l6-6 6 6",
    receive: "M12 5v14M6 13l6 6 6-6",
    extract: "M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4M12 3v12M7 10l5 5 5-5",
  };

  function percent(job: Job): number {
    if (job.bytesTotal > 0) return Math.min(100, (job.bytesDone / job.bytesTotal) * 100);
    if (job.filesTotal > 0) return Math.min(100, (job.filesDone / job.filesTotal) * 100);
    return 0;
  }

  let shown = $derived(jobs.visible(filter));
</script>

<section class="activity-drawer" class:open>
  <header class="activity-head">
    <span class="title">
      <RotateCwIcon class="size-4" />
      Activity
      {#if jobs.activeCount > 0}<span class="count">{jobs.activeCount}</span>{/if}
    </span>
    <div class="filters">
      {#each filters as item (item.id)}
        <button class:on={filter === item.id} onclick={() => (filter = item.id)}>{item.label}</button>
      {/each}
    </div>
    <div class="spacer"></div>
    <button class="ghost" onclick={() => jobs.clearCompleted()}>Clear completed</button>
    <button class="collapse" aria-label={open ? "Collapse activity" : "Open activity"} onclick={onToggle}>
      {#if open}<ChevronDownIcon class="size-4" />{:else}<ChevronUpIcon class="size-4" />{/if}
    </button>
  </header>

  {#if open}
    <div class="activity-body">
      {#if shown.length === 0}
        <p class="empty">No transfers.</p>
      {:else}
        {#each shown as job (job.id)}
          <article class="job" data-state={job.state}>
            <span class="kind">
              <svg viewBox="0 0 24 24"><path d={kindIcon[job.kind]} /></svg>
            </span>
            <div class="info">
              <div class="line">
                <span class="fname">{job.label || job.destination || job.kind}</span>
              </div>
              {#if job.destination}<div class="dest">→ {job.destination}</div>{/if}
              {#if isRunning(job)}
                <div class="bar"><span style="width: {percent(job)}%"></span></div>
                <div class="stats">
                  {#if job.bytesTotal > 0}
                    <span>{formatSize(job.bytesDone)} of {formatSize(job.bytesTotal)}</span>
                  {/if}
                  {#if job.filesTotal > 0}
                    <span>{job.filesDone} / {job.filesTotal} files</span>
                  {/if}
                </div>
              {:else if job.state === "failed"}
                <div class="stats error">{job.error ?? "Failed"}</div>
              {:else if job.state === "done"}
                <div class="stats ok">Completed</div>
              {:else}
                <div class="stats">Cancelled</div>
              {/if}
            </div>
            <div class="actions">
              {#if job.state === "done"}
                <span class="state-ok"><CheckIcon class="size-4" /></span>
                <button class="icon" aria-label="Remove" onclick={() => jobs.remove(job.id)}><XIcon class="size-4" /></button>
              {:else if job.state === "failed"}
                <span class="state-fail"><TriangleAlertIcon class="size-4" /></span>
                <button class="icon" aria-label="Dismiss" onclick={() => jobs.remove(job.id)}><XIcon class="size-4" /></button>
              {:else if isRunning(job)}
                <button class="icon" aria-label="Cancel" onclick={() => jobs.cancel(job.id)}><XIcon class="size-4" /></button>
              {:else}
                <button class="icon" aria-label="Dismiss" onclick={() => jobs.remove(job.id)}><XIcon class="size-4" /></button>
              {/if}
            </div>
          </article>
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .activity-drawer {
    display: flex;
    height: 44px;
    flex-shrink: 0;
    flex-direction: column;
    overflow: hidden;
    border-top: 1px solid #3a3734;
    background: #292725;
    transition: height 200ms cubic-bezier(0.2, 0, 0, 1);
  }
  .activity-drawer.open {
    height: 260px;
  }
  @media (prefers-reduced-motion: reduce) {
    .activity-drawer { transition: none; }
  }
  .activity-head {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 14px;
    border-bottom: 1px solid #3a3734;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    font-weight: 600;
    color: #f2f1f0;
  }
  .title :global(svg) {
    color: #0a9bff;
  }
  .count {
    display: grid;
    place-items: center;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 999px;
    background: #0a9bff;
    color: #04121f;
    font-size: 10.5px;
    font-weight: 700;
  }
  .filters {
    display: flex;
    gap: 3px;
    padding: 2px;
    border-radius: 8px;
    background: #1d1b1a;
  }
  .filters button {
    padding: 4px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: #a8a4a1;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .filters button:hover {
    color: #f2f1f0;
  }
  .filters button.on {
    background: #3b3836;
    color: #f2f1f0;
  }
  .spacer {
    flex: 1;
  }
  .ghost {
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: #a8a4a1;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .ghost:hover {
    background: #353230;
    color: #f2f1f0;
  }
  .collapse {
    display: grid;
    visibility: visible;
    flex-shrink: 0;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid #4a4642;
    border-radius: 6px;
    background: #34312e;
    color: #f2f1f0;
    cursor: pointer;
  }
  .collapse:hover {
    border-color: #0a9bff;
    background: #0a9bff;
    color: #04121f;
  }
  .activity-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 8px 12px;
  }
  .empty {
    padding: 30px 0;
    color: #8d8986;
    font-size: 12.5px;
    text-align: center;
  }
  .job {
    display: grid;
    grid-template-columns: 30px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px;
    border-radius: 9px;
  }
  .job:hover {
    background: #2e2b29;
  }
  .job + .job {
    border-top: 1px solid rgb(58 55 52 / 55%);
  }
  .kind {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: #34312e;
    color: #7cc4ff;
  }
  .kind svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .info {
    min-width: 0;
  }
  .fname {
    display: block;
    overflow: hidden;
    color: #eceae8;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dest {
    margin-top: 2px;
    overflow: hidden;
    color: #8d8986;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 4px;
    margin-top: 8px;
    overflow: hidden;
    border-radius: 999px;
    background: #3a3734;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: #0a9bff;
    transition: width 0.2s linear;
  }
  .stats {
    display: flex;
    gap: 10px;
    margin-top: 4px;
    color: #8d8986;
    font-size: 11px;
  }
  .stats.ok {
    color: #28c840;
  }
  .stats.error {
    display: block;
    color: #ff928b;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: #a8a4a1;
    cursor: pointer;
  }
  .icon:hover {
    background: #353230;
    color: #f2f1f0;
  }
  .state-ok {
    color: #28c840;
  }
  .state-fail {
    color: #ff5f57;
  }
</style>
