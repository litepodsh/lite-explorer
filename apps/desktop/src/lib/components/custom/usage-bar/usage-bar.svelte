<script lang="ts">
  import { sizeSegments, type UsageSegment } from "./segments.js";

  let {
    segments,
    total,
    height = 8,
    gap = 3,
    delay = 0,
    trackColor,
    label,
    /** Id of the segment currently emphasized by a hover elsewhere (e.g. a legend row). */
    activeId,
    /** Fired when the pointer enters or leaves a segment, with its id (or null). */
    onHover,
    class: className = "",
  }: {
    segments: UsageSegment[];
    total: number;
    /** Bar thickness in px. */
    height?: number;
    /** Space between segments in px. With a gap every segment is its own pill and the
     * unfilled part becomes a pill too; with 0 the segments share one rounded track. */
    gap?: number;
    /** Wait before the fill animation starts, in ms. Handy for staggering rows. */
    delay?: number;
    trackColor?: string;
    label?: string;
    activeId?: string;
    onHover?: (id: string | null) => void;
    class?: string;
  } = $props();

  const sized = $derived(sizeSegments(segments, total));
  const remainder = $derived(Math.max(0, 100 - sized.reduce((sum, segment) => sum + segment.percent, 0)));
</script>

<div
  class="usage-bar {className}"
  class:usage-bar-joined={gap === 0}
  role="img"
  aria-label={label}
  style:--usage-bar-height="{height}px"
  style:--usage-bar-gap="{gap}px"
  style:--usage-bar-delay="{delay}ms"
  style:--usage-bar-track={trackColor}>
  {#if total > 0}
    <div class="usage-bar-fill">
      {#each sized as segment (segment.id)}
        {#if segment.percent > 0}
          <span
            class="usage-bar-segment"
            class:is-active={activeId === segment.id}
            role="img"
            aria-label={segment.label}
            style:flex-grow={segment.percent}
            style:background={segment.color}
            title={segment.label}
            onpointerenter={() => onHover?.(segment.id)}
            onpointerleave={() => onHover?.(null)}></span>
        {/if}
      {/each}
      {#if remainder > 0}
        <span
          class="usage-bar-rest"
          class:is-active={activeId === "free"}
          role="img"
          aria-label="Free"
          style:flex-grow={remainder}
          onpointerenter={() => onHover?.("free")}
          onpointerleave={() => onHover?.(null)}></span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .usage-bar {
    --usage-bar-track: #3a3734;
    height: var(--usage-bar-height);
    border-radius: 999px;
  }
  .usage-bar-joined {
    --usage-bar-track: #1a1918;
    overflow: hidden;
    background: var(--usage-bar-track);
  }
  .usage-bar-fill {
    display: flex;
    gap: var(--usage-bar-gap);
    height: 100%;
    animation: usage-bar-reveal 900ms cubic-bezier(0.16, 1, 0.3, 1) var(--usage-bar-delay) both;
  }
  .usage-bar-segment,
  .usage-bar-rest {
    flex-basis: 0;
    height: 100%;
    border-radius: 999px;
    transition: flex-grow 500ms cubic-bezier(0.16, 1, 0.3, 1), filter 150ms ease, transform 150ms ease;
  }
  .usage-bar-segment {
    min-width: var(--usage-bar-height);
    background: #0a9bff;
  }
  .usage-bar-segment:hover {
    filter: brightness(1.15);
  }
  .usage-bar-segment.is-active {
    transform: scaleY(1.2);
  }
  .usage-bar-rest {
    background: var(--usage-bar-track);
  }
  .usage-bar-joined .usage-bar-segment {
    min-width: 0;
  }
  .usage-bar-joined .usage-bar-rest {
    background: transparent;
  }
  @keyframes usage-bar-reveal {
    from {
      clip-path: inset(0 100% 0 0 round 999px);
    }
    to {
      clip-path: inset(0 0 0 0 round 999px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .usage-bar-fill {
      animation: none;
    }
    .usage-bar-segment,
    .usage-bar-rest {
      transition: none;
    }
  }
</style>
