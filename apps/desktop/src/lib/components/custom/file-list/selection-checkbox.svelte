<script lang="ts">
  let {
    checked = false,
    indeterminate = false,
    label,
    onToggle,
  }: {
    checked?: boolean;
    indeterminate?: boolean;
    label: string;
    onToggle?: () => void;
  } = $props();

  // Rows mount and unmount while scrolling, so the pop only plays for a change after mount.
  let animate = $state(false);
  let first = true;
  $effect(() => {
    void checked;
    void indeterminate;
    if (first) first = false;
    else animate = true;
  });

  const status = $derived(checked ? "checked" : indeterminate ? "mixed" : "unchecked");
</script>

<!-- Clicks stay on the checkbox: they must not start a drag, select the row or open it.
     Keyboard toggling is handled by the file list (Space, Ctrl/Cmd+Space). -->
<span
  role="checkbox"
  aria-checked={checked ? "true" : indeterminate ? "mixed" : "false"}
  aria-label={label}
  tabindex="-1"
  class="selection-checkbox"
  data-state={status}
  data-animate={animate || undefined}
  onpointerdown={(event) => event.stopPropagation()}
  onmousedown={(event) => event.preventDefault()}
  onclick={(event) => {
    event.stopPropagation();
    onToggle?.();
  }}
  ondblclick={(event) => event.stopPropagation()}
  onkeydown={() => {}}>
  <svg viewBox="0 0 14 14" aria-hidden="true">
    <path class="check" d="M3.6 7.3 5.9 9.5 10.4 4.7" pathLength="1" />
    <path class="dash" d="M4 7h6" />
  </svg>
</span>

<style>
  .selection-checkbox {
    --ease: cubic-bezier(0.32, 0.72, 0, 1);
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 14px;
    height: 14px;
    flex: none;
    border-radius: 4px;
    background: rgb(255 255 255 / 0.04);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.18);
    color: #fff;
    cursor: default;
    transition:
      background-color 220ms var(--ease),
      box-shadow 220ms var(--ease),
      transform 160ms var(--ease);
  }

  /* Larger hit area than the drawn box. */
  .selection-checkbox::after {
    content: "";
    position: absolute;
    inset: -6px;
  }

  .selection-checkbox:hover {
    background: rgb(255 255 255 / 0.07);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.32);
  }

  .selection-checkbox:active {
    transform: scale(0.86);
  }

  .selection-checkbox[data-state="checked"],
  .selection-checkbox[data-state="mixed"] {
    background: #0a84ff;
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.22),
      inset 0 0 0 1px rgb(10 132 255 / 0.9),
      0 1px 3px rgb(10 132 255 / 0.28);
  }

  .selection-checkbox[data-state="checked"]:hover,
  .selection-checkbox[data-state="mixed"]:hover {
    background: #2b95ff;
  }

  svg {
    width: 14px;
    height: 14px;
    overflow: visible;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  /* Check draws in along its path and erases back out. */
  .check {
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    opacity: 0;
    /* Round caps still paint a dot at zero length, so the stroke hides once it has erased. */
    transition:
      stroke-dashoffset 140ms var(--ease),
      opacity 0ms linear 140ms;
  }

  [data-state="checked"] .check {
    stroke-dashoffset: 0;
    opacity: 1;
    transition: stroke-dashoffset 240ms var(--ease) 40ms;
  }

  .dash {
    transform-origin: 7px 7px;
    transform: scaleX(0);
    opacity: 0;
    transition:
      transform 200ms var(--ease),
      opacity 120ms var(--ease);
  }

  [data-state="mixed"] .dash {
    transform: scaleX(1);
    opacity: 1;
  }

  [data-animate][data-state="checked"],
  [data-animate][data-state="mixed"] {
    animation: pop 320ms var(--ease);
  }

  [data-animate][data-state="unchecked"] {
    animation: release 260ms var(--ease);
  }

  @keyframes pop {
    0% {
      transform: scale(0.82);
    }
    55% {
      transform: scale(1.1);
    }
    100% {
      transform: scale(1);
    }
  }

  @keyframes release {
    0% {
      transform: scale(0.9);
    }
    60% {
      transform: scale(1.04);
    }
    100% {
      transform: scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .selection-checkbox,
    .check,
    .dash {
      transition: none;
    }
    [data-animate] {
      animation: none !important;
    }
  }
</style>
