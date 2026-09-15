<script lang="ts">
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import type { Snippet } from "svelte";

  type Props = {
    /** Tooltip text and accessible name, e.g. "Play" or "Back 10 seconds". */
    label: string;
    onclick?: (event: MouseEvent) => void;
    disabled?: boolean;
    /** Portals the tooltip here so it stays visible when the player is fullscreen. */
    portalTarget?: HTMLElement | null;
    children: Snippet;
  };
  let { label, onclick, disabled = false, portalTarget = null, children }: Props = $props();
</script>

<Tooltip.Root delayDuration={400}>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <button {...props} type="button" class="media-tool" aria-label={label} {disabled} onclick={onclick}>
        {@render children()}
      </button>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content
    side="top"
    sideOffset={10}
    portalProps={portalTarget ? { to: portalTarget } : undefined}>{label}</Tooltip.Content>
</Tooltip.Root>

<style>
  .media-tool {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: #d9d6d3;
    transition:
      background-color 200ms cubic-bezier(0.32, 0.72, 0, 1),
      color 200ms cubic-bezier(0.32, 0.72, 0, 1),
      transform 200ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .media-tool:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.09);
    color: #f2f1f0;
  }
  .media-tool:active:not(:disabled) {
    transform: scale(0.93);
  }
  .media-tool:disabled {
    color: #615d5a;
  }
  @media (prefers-reduced-motion: reduce) {
    .media-tool {
      transition: none;
    }
  }
</style>
