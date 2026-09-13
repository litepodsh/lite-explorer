<script lang="ts">
  // Press-and-hold button, ported from litepod's hold-to-delete: a fill sweeps across
  // while held and the action fires only when it completes. Space/Enter hold works too.
  import { onDestroy, type Component } from "svelte";
  import CheckIcon from "@lucide/svelte/icons/check";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import { HoldGesture, isKeyPressed } from "./hold-gesture.js";

  let {
    label = "Hold to delete",
    doneLabel = "Deleting…",
    holdMs = 1100,
    disabled = false,
    variant = "destructive",
    icon,
    globalHoldKeys = [],
    onconfirm,
  }: {
    label?: string;
    doneLabel?: string;
    holdMs?: number;
    disabled?: boolean;
    variant?: "destructive" | "primary";
    icon?: Component;
    /** Physical keys that drive the hold from anywhere while this button is mounted. */
    globalHoldKeys?: string[];
    onconfirm: () => void;
  } = $props();

  let holding = $state(false);
  let confirmed = $state(false);
  const ActionIcon = $derived(icon ?? (variant === "destructive" ? Trash2Icon : CheckIcon));
  const active = $derived(holding || confirmed);

  const gesture = new HoldGesture(
    // Read once: changing the duration mid-hold would desync the fill from the timer.
    // svelte-ignore state_referenced_locally
    holdMs,
    () => onconfirm(),
    (state) => {
      holding = state.holding;
      confirmed = state.confirmed;
    },
  );

  function start() {
    if (!disabled) gesture.start();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.repeat || (event.key !== " " && event.key !== "Enter")) return;
    event.preventDefault();
    start();
  }

  function onkeyup(event: KeyboardEvent) {
    if (event.key === " " || event.key === "Enter") gesture.cancel();
  }

  function isEditable(target: EventTarget | null): boolean {
    return (
      target instanceof Element &&
      target.closest("input, textarea, select, [contenteditable='true']") !== null
    );
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (disabled || !globalHoldKeys.includes(event.key) || isEditable(event.target)) return;
    event.preventDefault();
    start();
  }

  function onWindowKeyup(event: KeyboardEvent) {
    if (globalHoldKeys.includes(event.key)) gesture.cancel();
  }

  // Starts right away when the key that opened the sheet is still held down.
  $effect(() => {
    if (disabled || globalHoldKeys.length === 0) return;
    if (globalHoldKeys.some(isKeyPressed)) gesture.start();
  });

  onDestroy(() => gesture.cancel());
</script>

<svelte:window onkeydown={onWindowKeydown} onkeyup={onWindowKeyup} />

<button
  type="button"
  class="hold {variant}"
  data-holding={holding || undefined}
  data-confirmed={confirmed || undefined}
  aria-label={confirmed ? doneLabel : label}
  style:--hold-ms={`${holdMs}ms`}
  {disabled}
  onpointerdown={start}
  onpointerup={() => gesture.cancel()}
  onpointerleave={() => gesture.cancel()}
  onpointercancel={() => gesture.cancel()}
  {onkeydown}
  {onkeyup}
  oncontextmenu={(event) => event.preventDefault()}>
  <span class="face">
    <ActionIcon aria-hidden="true" />
    {label}
  </span>
  <span class="fill" class:active aria-hidden="true">
    <span class="fill-face" class:active>
      {#if confirmed}<CheckIcon />{:else}<ActionIcon />{/if}
      {confirmed ? doneLabel : label}
    </span>
  </span>
  <span class="bevel" class:active aria-hidden="true"></span>
</button>

<style>
  .hold {
    --hold-ease: cubic-bezier(0.32, 0.72, 0, 1);
    --hold-tone: #ff453a;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 148px;
    height: 28px;
    padding: 0 12px;
    overflow: hidden;
    border: 0;
    /* Idle look matches DialogButton (Cancel); the tone only shows in the icon and fill. */
    border-radius: 7px;
    background: #33302e;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.12);
    color: #eceae8;
    font: inherit;
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
    touch-action: none;
    user-select: none;
    transition:
      transform 160ms var(--hold-ease),
      background-color 160ms var(--hold-ease);
  }
  .hold:hover {
    background: #3b3836;
  }
  .face :global(svg) {
    color: var(--hold-tone);
  }
  .hold.primary {
    --hold-tone: #0a9bff;
  }
  .hold:focus-visible {
    outline: 2px solid rgb(10 155 255 / 0.6);
    outline-offset: 1px;
  }
  .hold:active {
    transform: scale(0.97);
  }
  .hold:disabled {
    opacity: 0.5;
    pointer-events: none;
  }
  .face,
  .fill-face {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .hold :global(svg) {
    width: 14px;
    height: 14px;
    stroke-width: 2;
  }
  .fill {
    position: absolute;
    inset: 0;
    z-index: 1;
    overflow: hidden;
    background: var(--hold-tone);
    transform: translateX(-100%);
    transition: transform 300ms var(--hold-ease);
  }
  .fill-face {
    position: absolute;
    inset: 0;
    justify-content: center;
    color: #fff;
    transform: translateX(100%);
    transition: transform 300ms var(--hold-ease);
  }
  .fill.active,
  .fill-face.active {
    transform: translateX(0);
    transition: transform var(--hold-ms) linear;
  }
  .bevel {
    position: absolute;
    inset: 0;
    z-index: 2;
    border-radius: inherit;
    pointer-events: none;
    transition: box-shadow 300ms var(--hold-ease);
  }
  .bevel.active {
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.22),
      inset 0 1px 3px rgb(0 0 0 / 0.2);
  }
  @media (prefers-reduced-motion: reduce) {
    .fill,
    .fill-face,
    .fill.active,
    .fill-face.active {
      transition: none;
    }
  }
</style>
