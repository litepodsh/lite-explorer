<script lang="ts" generics="T extends string">
  // A small row of mutually exclusive options.
  let {
    options,
    value,
    label,
    onchange,
  }: {
    options: readonly { value: T; label: string }[];
    value: T;
    label: string;
    onchange: (value: T) => void;
  } = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={value === option.value}
      class:active={value === option.value}
      onclick={() => onchange(option.value)}>
      {option.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 2px;
    border-radius: calc(var(--app-radius) - 4px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  button {
    flex: 1;
    min-width: 40px;
    height: 24px;
    padding: 0 8px;
    border: 0;
    border-radius: calc(var(--app-radius) - 6px);
    background: transparent;
    color: var(--app-fg-muted);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    transition:
      background-color 160ms cubic-bezier(0.32, 0.72, 0, 1),
      color 160ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  button:hover {
    color: var(--app-fg);
  }
  button.active {
    background: var(--app-surface-raised);
    box-shadow: inset 0 0 0 1px var(--app-border-strong);
    color: var(--app-fg);
  }
  button:focus-visible {
    outline: 2px solid var(--app-accent);
    outline-offset: 1px;
  }
</style>
