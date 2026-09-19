<script lang="ts">
  import CheckIcon from "@lucide/svelte/icons/check";
  import { formatJobDuration, type Job } from "$lib/transfers/jobs.js";
  import { etaSeconds, formatBytes, formatEta, progressFraction } from "$lib/transfers/progress.js";
  import { overflowCount, shouldAutoDismiss, visibleToasts } from "$lib/transfers/toasts.js";
  import type { JobsStore } from "$lib/transfers/jobs.svelte.js";

  let { jobs }: { jobs: JobsStore } = $props();

  let now = $state(Date.now());
  let dismissed = $state<Record<string, true>>({});

  $effect(() => {
    const timer = setInterval(() => {
      now = Date.now();
      for (const job of jobs.jobs) {
        if (shouldAutoDismiss(job, now)) dismissed[job.id] = true;
      }
    }, 500);
    return () => clearInterval(timer);
  });

  const shown = $derived(visibleToasts(jobs.jobs, dismissed));
  const overflow = $derived(overflowCount(jobs.jobs, dismissed));

  function fractionOf(job: Job): number {
    if (job.state === "done") return 1;
    return (
      progressFraction(job.bytesDone, job.bytesTotal) ??
      progressFraction(job.filesDone, job.filesTotal) ??
      0
    );
  }

  function etaOf(job: Job): string | null {
    const elapsed = (now - job.startedAt) / 1000;
    const byBytes = etaSeconds(job.bytesDone, job.bytesTotal, elapsed);
    return formatEta(byBytes ?? etaSeconds(job.filesDone, job.filesTotal, elapsed));
  }
</script>

<div class="toast-stack">
  {#each shown as job (job.id)}
    {@const fraction = fractionOf(job)}
    {@const eta = etaOf(job)}
    <article
      class="toast"
      class:toast-done={job.state === "done"}
      class:toast-cancelled={job.state === "cancelled"}
      class:toast-failed={job.state === "failed"}>
      <div class="core">
        <header>
          <span class="label">{job.label}</span>
          {#if job.state === "done"}
            <span class="check" aria-hidden="true"><CheckIcon /></span>
          {/if}
          <button class="dismiss" aria-label="Dismiss" onclick={() => (dismissed[job.id] = true)}>×</button>
        </header>
        <div class="bar" role="progressbar" aria-valuenow={Math.round(fraction * 100)}>
          <div class="fill" style:transform="scaleX({fraction})"></div>
        </div>
        <footer>
          <span class="tabular-nums">{job.filesDone} / {job.filesTotal} files</span>
          {#if job.bytesTotal > 0}
            <span class="tabular-nums">{formatBytes(job.bytesDone)} / {formatBytes(job.bytesTotal)}</span>
          {:else if job.bytesDone > 0}
            <span class="tabular-nums">{formatBytes(job.bytesDone)}</span>
          {/if}
          <span class="tabular-nums">{formatJobDuration(job, now)}</span>
          {#if eta}<span class="tabular-nums">{eta}</span>{/if}
          {#if job.error}<span class="error">{job.error}</span>{/if}
          {#if job.state === "active" && job.cancellable}
            <button class="cancel" onclick={() => jobs.cancel(job.id)}>Cancel</button>
          {/if}
        </footer>
      </div>
    </article>
  {/each}
  {#if overflow > 0}<p class="overflow">{overflow} more</p>{/if}
</div>

<style>
  .toast-stack {
    position: fixed;
    right: 1rem;
    bottom: 2.5rem;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 22rem;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    padding: 0.375rem;
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.05);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.1);
    animation: toast-in 500ms cubic-bezier(0.32, 0.72, 0, 1) both;
  }
  .core {
    border-radius: calc(1rem - 0.375rem);
    background: #1a1a1c;
    box-shadow: inset 0 1px 1px rgba(255, 255, 255, 0.08);
    padding: 0.75rem;
    color: #e7e5e4;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    margin: 0.625rem 0 0.5rem;
    height: 0.25rem;
    border-radius: 9999px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    transform-origin: left;
    background: #0a9bff;
    transition:
      transform 500ms cubic-bezier(0.32, 0.72, 0, 1),
      background-color 400ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .toast-done .core {
    box-shadow:
      inset 0 1px 1px rgba(255, 255, 255, 0.08),
      0 0 0 1px rgba(52, 211, 153, 0.4);
  }
  .toast-done .fill {
    background: #34d399;
  }
  .toast-cancelled .fill {
    background: #9c9895;
  }
  .toast-failed .fill {
    background: #ff6b5e;
  }
  .check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.125rem;
    height: 1.125rem;
    border-radius: 9999px;
    color: #0b1f17;
    background: #34d399;
    animation: check-pop 420ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  .check :global(svg) {
    width: 0.75rem;
    height: 0.75rem;
    stroke-width: 3;
  }
  @keyframes check-pop {
    0% {
      opacity: 0;
      transform: scale(0.3) rotate(-14deg);
    }
    60% {
      opacity: 1;
      transform: scale(1.15) rotate(5deg);
    }
    100% {
      opacity: 1;
      transform: scale(1) rotate(0deg);
    }
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 0.75rem;
    font-size: 0.6875rem;
    color: #9c9895;
  }
  .error {
    color: #ff6b6b;
  }
  .dismiss,
  .cancel {
    background: none;
    border: 0;
    color: #9c9895;
    cursor: pointer;
    font: inherit;
  }
  .cancel {
    color: #0a9bff;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(0.75rem);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
    .check {
      animation: none;
    }
  }
</style>
