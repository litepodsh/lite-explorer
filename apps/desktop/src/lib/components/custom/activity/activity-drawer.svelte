<script lang="ts">
  import UploadIcon from "@lucide/svelte/icons/upload";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FolderInputIcon from "@lucide/svelte/icons/folder-input";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import FilePlusIcon from "@lucide/svelte/icons/file-plus";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import RotateCwIcon from "@lucide/svelte/icons/rotate-cw";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import XIcon from "@lucide/svelte/icons/x";
  import { formatSize } from "$lib/components/custom/preview/format.js";
  import type { JobsStore } from "$lib/transfers/jobs.svelte.js";
  import { formatJobDuration, isRunning, type Job, type JobFilter, type JobKind } from "$lib/transfers/jobs.js";

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
  let now = $state(Date.now());
  const timestamp = new Intl.DateTimeFormat(undefined, {
    year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", second: "2-digit",
    fractionalSecondDigits: 3, hourCycle: "h23", timeZoneName: "short",
  });

  $effect(() => {
    if (!open || jobs.activeCount === 0) return;
    now = Date.now();
    const timer = setInterval(() => { now = Date.now(); }, 250);
    return () => clearInterval(timer);
  });

  const filters: { id: JobFilter; label: string }[] = [
    { id: "all", label: "All" },
    { id: "active", label: "Active" },
    { id: "done", label: "Done" },
    { id: "failed", label: "Failed" },
  ];

  const kindIcon: Record<JobKind, typeof CopyIcon> = {
    upload: UploadIcon,
    download: DownloadIcon,
    copy: CopyIcon,
    move: FolderInputIcon,
    delete: Trash2Icon,
    send: UploadIcon,
    receive: DownloadIcon,
    extract: ArchiveIcon,
    compress: ArchiveIcon,
    create: FilePlusIcon,
    rename: PencilIcon,
    action: RotateCwIcon,
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
        <p class="empty">No activities.</p>
      {:else}
        {#each shown as job (job.id)}
          {@const Icon = kindIcon[job.kind]}
          <article class="job" data-state={job.state}>
            <span class="kind">
              <Icon />
            </span>
            <div class="info">
              <div class="line">
                <span class="fname">{job.label || job.destination || job.kind}</span>
              </div>
              {#if job.destination}<div class="dest">→ {job.destination}</div>{/if}
              {#if isRunning(job)}
                {#if job.bytesTotal > 0 || job.filesTotal > 0}
                  <div class="bar"><span style="width: {percent(job)}%"></span></div>
                {:else}
                  <div class="stats">In progress…</div>
                {/if}
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
              <div class="timing">
                <span>Started <time datetime={new Date(job.startedAt).toISOString()}>{timestamp.format(job.startedAt)}</time></span>
                {#if job.finishedAt !== undefined}
                  <span>Finished <time datetime={new Date(job.finishedAt).toISOString()}>{timestamp.format(job.finishedAt)}</time></span>
                {/if}
                <span>{isRunning(job) ? "Elapsed" : "Duration"} {formatJobDuration(job, now)}</span>
              </div>
            </div>
            <div class="actions">
              {#if job.state === "done"}
                <span class="state-ok"><CheckIcon class="size-4" /></span>
                <button class="icon" aria-label="Remove" onclick={() => jobs.remove(job.id)}><XIcon class="size-4" /></button>
              {:else if job.state === "failed"}
                <span class="state-fail"><TriangleAlertIcon class="size-4" /></span>
                <button class="icon" aria-label="Dismiss" onclick={() => jobs.remove(job.id)}><XIcon class="size-4" /></button>
              {:else if isRunning(job) && job.cancellable !== false}
                <button class="icon" aria-label="Cancel" onclick={() => jobs.cancel(job.id)}><XIcon class="size-4" /></button>
              {:else if !isRunning(job)}
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
    border-top: 1px solid var(--app-border);
    background: var(--app-sidebar);
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
    border-bottom: 1px solid var(--app-border);
  }
  .title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    font-weight: 600;
    color: var(--app-fg);
  }
  .title :global(svg) {
    color: var(--app-accent);
  }
  .count {
    display: grid;
    place-items: center;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--app-accent);
    color: var(--app-accent-fg);
    font-size: 10.5px;
    font-weight: 700;
  }
  .filters {
    display: flex;
    gap: 3px;
    padding: 2px;
    border-radius: 8px;
    background: var(--app-input);
  }
  .filters button {
    padding: 4px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--app-fg-muted);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .filters button:hover {
    color: var(--app-fg);
  }
  .filters button.on {
    background: var(--app-surface-raised);
    color: var(--app-fg);
  }
  .spacer {
    flex: 1;
  }
  .ghost {
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--app-fg-muted);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .ghost:hover {
    background: var(--app-surface-raised);
    color: var(--app-fg);
  }
  .collapse {
    display: grid;
    visibility: visible;
    flex-shrink: 0;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--app-border);
    border-radius: calc(var(--app-radius) - 5px);
    background: var(--app-surface-raised);
    color: var(--app-fg);
    cursor: pointer;
  }
  .collapse:hover {
    border-color: var(--app-accent);
    background: var(--app-accent);
    color: var(--app-accent-fg);
  }
  .activity-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 8px 12px;
  }
  .empty {
    padding: 30px 0;
    color: var(--app-fg-muted);
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
    background: var(--app-surface-raised);
  }
  .job + .job {
    border-top: 1px solid var(--app-border);
  }
  .kind {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--app-surface-raised);
    color: var(--app-accent);
  }
  .kind :global(svg) {
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
    color: var(--app-fg);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dest {
    margin-top: 2px;
    overflow: hidden;
    color: var(--app-fg-muted);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 4px;
    margin-top: 8px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--app-input);
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--app-accent);
    transition: width 0.2s linear;
  }
  .stats {
    display: flex;
    gap: 10px;
    margin-top: 4px;
    color: var(--app-fg-muted);
    font-size: 11px;
  }
  .stats.ok {
    color: #28c840;
  }
  .stats.error {
    display: block;
    color: #ff928b;
  }
  .timing {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 14px;
    margin-top: 6px;
    color: var(--app-fg-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
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
    color: var(--app-fg-muted);
    cursor: pointer;
  }
  .icon:hover {
    background: var(--app-surface-raised);
    color: var(--app-fg);
  }
  .state-ok {
    color: #28c840;
  }
  .state-fail {
    color: #ff5f57;
  }
</style>
