export type SwipeDirection = "back" | "forward";

/** Fraction of the list width the drag must pass to commit on release. */
export const SWIPE_COMMIT = 0.25;

/** How much movement is allowed toward a side without history (rubber band). */
const RESISTANCE = 0.22;

export function directionFor(amount: number): SwipeDirection {
  return amount > 0 ? "back" : "forward";
}

export function isAvailable(
  direction: SwipeDirection,
  canBack: boolean,
  canForward: boolean,
): boolean {
  return direction === "back" ? canBack : canForward;
}

export function shouldCommit(amount: number, canBack: boolean, canForward: boolean): boolean {
  return isAvailable(directionFor(amount), canBack, canForward) && Math.abs(amount) >= SWIPE_COMMIT;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/**
 * Transforms for the current and incoming listing layers, in percent of the
 * layer width. Positive amount (fingers right) is back, revealing the previous
 * folder from the left; negative is forward.
 */
export function swipeTransforms(
  amount: number,
  canBack: boolean,
  canForward: boolean,
): { current: number; incoming: number } {
  if (amount === 0) return { current: 0, incoming: 0 };
  const available = isAvailable(directionFor(amount), canBack, canForward);
  const damped = available ? amount : amount * RESISTANCE;
  const current = clamp(damped, -1, 1) * 100;
  const incoming = current + (amount > 0 ? -100 : 100);
  return { current, incoming };
}

export function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}
