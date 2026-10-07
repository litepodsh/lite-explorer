<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";

  let {
    variant = "secondary",
    children,
    ...rest
  }: HTMLButtonAttributes & { variant?: "secondary" | "primary" } = $props();
</script>

<button type="button" class="dialog-button {variant}" {...rest}>{@render children?.()}</button>

<style>
  .dialog-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 7px;
    background: var(--app-surface-raised);
    box-shadow: inset 0 0 0 1px var(--app-border-strong);
    color: var(--app-fg);
    font: inherit;
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
    transition:
      transform 160ms cubic-bezier(0.32, 0.72, 0, 1),
      background-color 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .dialog-button:hover {
    background: var(--app-hover-strong);
  }
  .dialog-button:active {
    transform: scale(0.97);
  }
  .dialog-button:focus-visible {
    outline: 2px solid var(--app-accent);
    outline-offset: 1px;
  }
  .dialog-button:disabled {
    opacity: 0.45;
    pointer-events: none;
  }
  .dialog-button.primary {
    background: var(--app-accent);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.22);
    color: var(--app-accent-fg);
  }
  .dialog-button.primary:hover {
    background: color-mix(in srgb, var(--app-accent) 82%, white);
  }
</style>
