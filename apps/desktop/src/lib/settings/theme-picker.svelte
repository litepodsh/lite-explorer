<script lang="ts">
  import type { Theme } from "./settings.js";

  type Option = { value: Theme; label: string; description: string; swatch: string };
  const options: Option[] = [
    { value: "off-white", label: "Off White", description: "Soft paper", swatch: "off-white" },
    { value: "light", label: "Light", description: "Clean daylight", swatch: "light" },
    { value: "dark", label: "Dark", description: "Current look", swatch: "dark" },
    { value: "off-black", label: "Off Black", description: "Soft black", swatch: "off-black" },
    { value: "oled", label: "OLED Black", description: "Pure black and white", swatch: "oled" },
  ];

  let { value, disabled = false, onchange }: { value: Theme; disabled?: boolean; onchange: (value: Theme) => void } = $props();
  const id = $props.id();
  let buttons = $state<HTMLButtonElement[]>([]);

  function move(event: KeyboardEvent, index: number) {
    const delta = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 0;
    if (!delta || disabled) return;
    event.preventDefault();
    const next = (index + delta + options.length) % options.length;
    onchange(options[next].value);
    buttons[next]?.focus();
  }
</script>

<div class="picker" class:disabled>
  <div class="copy">
    <span id={`${id}-label`} class="label">Theme</span>
    <p>Choose the materials used across every Lite Explorer window.</p>
  </div>
  <div class="choices" role="radiogroup" aria-labelledby={`${id}-label`} aria-disabled={disabled}>
    {#each options as option, index (option.value)}
      <button
        bind:this={buttons[index]}
        type="button"
        role="radio"
        aria-checked={option.value === value}
        tabindex={option.value === value ? 0 : -1}
        {disabled}
        onclick={() => onchange(option.value)}
        onkeydown={(event) => move(event, index)}>
        <span class={`swatch ${option.swatch}`} aria-hidden="true"><span></span></span>
        <span class="option-copy"><strong>{option.label}</strong><small>{option.description}</small></span>
      </button>
    {/each}
  </div>
</div>

<style>
  .picker, .copy { min-width: 0; }
  .label, strong { color: var(--app-fg); font-size: 12.5px; font-weight: 600; }
  p, small { color: var(--app-fg-muted); font-size: 11.5px; line-height: 1.4; }
  p { margin: 3px 0 10px; }
  .choices { display: grid; grid-template-columns: repeat(auto-fit, minmax(104px, 1fr)); gap: 7px; }
  button { display: flex; min-width: 0; flex-direction: column; gap: 7px; padding: 6px; border: 0; border-radius: calc(var(--app-radius) - 3px); background: transparent; text-align: left; transition: transform 180ms cubic-bezier(0.32, 0.72, 0, 1), background-color 180ms cubic-bezier(0.32, 0.72, 0, 1); }
  button:hover:not(:disabled) { background: color-mix(in srgb, var(--app-fg) 7%, transparent); transform: translateY(-1px); }
  button:active:not(:disabled) { transform: scale(.98); }
  button[aria-checked="true"] { background: color-mix(in srgb, var(--app-accent) 16%, transparent); }
  button:focus-visible { outline: 2px solid var(--app-accent); outline-offset: 2px; }
  .swatch { position: relative; display: block; height: 42px; overflow: hidden; border: 1px solid rgb(127 127 127 / 28%); border-radius: calc(var(--app-radius) - 5px); background: var(--swatch-bg); box-shadow: inset 0 1px rgb(255 255 255 / 18%); }
  .swatch span { position: absolute; inset: 9px 7px 7px; border-radius: calc(var(--app-radius) - 8px); background: var(--swatch-surface); box-shadow: 0 5px 10px -6px var(--swatch-shadow); }
  .off-white { --swatch-bg: #f4f1ed; --swatch-surface: #fff; --swatch-shadow: #655a4e; }
  .light { --swatch-bg: #f7f7f8; --swatch-surface: #fff; --swatch-shadow: #4f5761; }
  .dark { --swatch-bg: #242220; --swatch-surface: #3a3734; --swatch-shadow: #000; }
  .off-black { --swatch-bg: #121212; --swatch-surface: #282828; --swatch-shadow: #000; }
  .oled { --swatch-bg: #000; --swatch-surface: #111; --swatch-shadow: #fff; }
  .option-copy { display: grid; min-width: 0; gap: 1px; }
  strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; }
  small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 10px; }
  .disabled { opacity: .55; }
  @media (prefers-reduced-motion: reduce) { button { transition: none; } }
</style>
