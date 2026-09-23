<script lang="ts" generics="T extends string | number">
  let {
    label,
    description,
    options,
    value,
    onchange,
  }: {
    label: string;
    description?: string;
    options: { value: T; label: string }[];
    value: T;
    onchange: (value: T) => void;
  } = $props();

  const id = $props.id();
  let buttons = $state<HTMLButtonElement[]>([]);

  /** Arrow keys choose the previous or next option, like a native radio group. */
  function move(event: KeyboardEvent, index: number) {
    const delta = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 0;
    if (delta === 0) return;
    event.preventDefault();
    const next = (index + delta + options.length) % options.length;
    onchange(options[next].value);
    buttons[next]?.focus();
  }
</script>

<div class="row">
  <div class="copy">
    <span id={`${id}-label`} class="label">{label}</span>
    {#if description}<p>{description}</p>{/if}
  </div>
  <div class="segments" role="radiogroup" aria-labelledby={`${id}-label`}>
    {#each options as option, index (option.value)}
      <button
        bind:this={buttons[index]}
        type="button"
        role="radio"
        aria-checked={option.value === value}
        tabindex={option.value === value ? 0 : -1}
        onclick={() => onchange(option.value)}
        onkeydown={(event) => move(event, index)}>{option.label}</button>
    {/each}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .copy {
    min-width: 0;
  }
  .label {
    color: var(--app-fg);
    font-size: 12.5px;
    font-weight: 500;
  }
  p {
    margin: 3px 0 0;
    color: var(--app-fg-muted);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .segments {
    display: inline-flex;
    flex-shrink: 0;
    gap: 2px;
    padding: 2px;
    border-radius: calc(var(--app-radius) - 3px);
    background: var(--app-input);
    box-shadow: inset 0 0 0 1px var(--app-border);
  }
  button {
    min-width: 56px;
    padding: 4px 10px;
    border: 0;
    border-radius: calc(var(--app-radius) - 5px);
    background: transparent;
    color: var(--app-fg-muted);
    font-size: 12px;
    font-weight: 500;
    transition:
      background-color 180ms cubic-bezier(0.32, 0.72, 0, 1),
      color 180ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  button[aria-checked="true"] {
    background: var(--app-surface-raised);
    color: var(--app-fg);
    box-shadow: inset 0 1px color-mix(in srgb, var(--app-fg) 12%, transparent);
  }
  button:focus-visible {
    outline: 2px solid var(--app-accent);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    button {
      transition: none;
    }
  }
</style>
