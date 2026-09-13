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
    background: #33302e;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.12);
    color: #eceae8;
    font: inherit;
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
    transition:
      transform 160ms cubic-bezier(0.32, 0.72, 0, 1),
      background-color 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  .dialog-button:hover {
    background: #3b3836;
  }
  .dialog-button:active {
    transform: scale(0.97);
  }
  .dialog-button:focus-visible {
    outline: 2px solid rgb(10 155 255 / 0.6);
    outline-offset: 1px;
  }
  .dialog-button:disabled {
    opacity: 0.45;
    pointer-events: none;
  }
  .dialog-button.primary {
    background: #0a9bff;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.22);
    color: #fff;
  }
  .dialog-button.primary:hover {
    background: #2aa8ff;
  }
</style>
