import { quartOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

/** Checkboxes grow in from the icon side when turned on and shrink back when turned off. */
export function checkboxReveal(
  _node: Element,
  { delay = 0 }: { delay?: number } = {},
): TransitionConfig {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  return {
    delay: reduced ? 0 : delay,
    duration: reduced ? 0 : 260,
    easing: quartOut,
    css: (t) =>
      `opacity: ${t}; transform: translateX(${(1 - t) * -5}px) scale(${0.55 + 0.45 * t});`,
  };
}
