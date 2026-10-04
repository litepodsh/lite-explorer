<script lang="ts">
  import { Switch as SwitchPrimitive } from "bits-ui";
  import { cn, type WithoutChildrenOrChild } from "#lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    checked = $bindable(false),
    size = "default",
    ...restProps
  }: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
    size?: "sm" | "default";
  } = $props();

  let thumbSurface = $state<HTMLSpanElement | null>(null);
  let previousChecked: boolean | undefined;

  $effect(() => {
    const isChecked = checked === true;
    if (!thumbSurface || previousChecked === undefined) {
      previousChecked = isChecked;
      return;
    }
    if (previousChecked === isChecked) return;
    previousChecked = isChecked;
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    thumbSurface.getAnimations().forEach((animation) => animation.cancel());
    thumbSurface.animate(
      [{ transform: "scale(1)" }, { transform: "scale(1.2)", offset: 0.45 }, { transform: "scale(1)" }],
      { duration: 364, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
    );
  });
</script>

<SwitchPrimitive.Root
  bind:ref
  bind:checked
  data-slot="soft-switch"
  data-size={size}
  class={cn(
    "peer relative inline-flex shrink-0 outline-none focus-visible:ring-3 focus-visible:ring-[var(--app-accent)]/30 aria-invalid:ring-3 aria-invalid:ring-destructive/20 after:absolute after:-inset-x-3 after:-inset-y-2 data-disabled:cursor-not-allowed data-disabled:opacity-50",
    className,
  )}
  {...restProps}>
  <SwitchPrimitive.Thumb data-slot="soft-switch-thumb" class="pointer-events-none absolute rounded-full">
    <span bind:this={thumbSurface} data-slot="soft-switch-thumb-surface"></span>
  </SwitchPrimitive.Thumb>
</SwitchPrimitive.Root>

<style>
  :global([data-slot="soft-switch"]) {
    --soft-track-width: 2.75rem;
    --soft-track-height: 1.5rem;
    --soft-thumb-size: 1rem;
    --soft-inset: 0.25rem;
    width: var(--soft-track-width);
    height: var(--soft-track-height);
    border-radius: 999px;
    background-color: var(--app-surface-raised);
    background-image: linear-gradient(rgb(0 0 0 / 7%), rgb(255 255 255 / 0));
    box-shadow: inset 0 0.07em 0.1em -0.1em rgb(0 0 0 / 40%), inset 0 0.05em 0.08em -0.01em rgb(255 255 255 / 35%);
    transition: box-shadow 364ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  :global([data-slot="soft-switch"]::before) {
    position: absolute; inset: 0; z-index: 0; border-radius: inherit;
    background: linear-gradient(rgb(0 0 0 / 7%), rgb(255 255 255 / 0)), var(--app-accent);
    content: ""; opacity: 0; pointer-events: none;
    transition: opacity 364ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  :global([data-slot="soft-switch"][data-size="sm"]) {
    --soft-track-width: 2rem; --soft-track-height: 1.25rem; --soft-thumb-size: 0.75rem;
  }
  :global([data-slot="soft-switch"][data-state="checked"]::before) { opacity: 1; }
  :global([data-slot="soft-switch-thumb"]) {
    z-index: 1; top: var(--soft-inset); left: var(--soft-inset);
    width: var(--soft-thumb-size); height: var(--soft-thumb-size);
    transition: transform 364ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  :global([data-slot="soft-switch-thumb-surface"]) {
    display: block; width: 100%; height: 100%; border-radius: inherit;
    background: linear-gradient(to bottom, color-mix(in srgb, var(--app-surface) 92%, white) 10%, var(--app-surface));
    box-shadow: inset 0 0.1em 0.15em -0.05em rgb(255 255 255 / 80%), 0 0.5em 0.3em -0.1em rgb(0 0 0 / 25%);
  }
  :global([data-slot="soft-switch-thumb"][data-state="checked"]) {
    transform: translateX(calc(var(--soft-track-width) - var(--soft-thumb-size) - (var(--soft-inset) * 2)));
  }
  :global([dir="rtl"] [data-slot="soft-switch-thumb"][data-state="checked"]) { transform: translateX(-1.25rem); }
  :global([dir="rtl"] [data-slot="soft-switch"][data-size="sm"] [data-slot="soft-switch-thumb"][data-state="checked"]) { transform: translateX(-0.75rem); }
</style>
