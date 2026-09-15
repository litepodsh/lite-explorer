/** Distance the pointer travels before a press becomes a drag. */
export const DRAG_THRESHOLD = 4;

type PointerDragCallbacks = {
  onStart?: (event: PointerEvent) => void;
  onMove: (event: PointerEvent) => void;
  onDrop: (event: PointerEvent) => void;
  onCancel: () => void;
};

/** True once the pointer is far enough from where the press started to count as a drag. */
export function passedThreshold(
  from: { x: number; y: number },
  to: { x: number; y: number },
): boolean {
  return Math.hypot(to.x - from.x, to.y - from.y) >= DRAG_THRESHOLD;
}

/** Stops the click that follows a drag's pointerup from selecting or opening what is under it. */
function swallowNextClick() {
  const swallow = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
  };
  window.addEventListener("click", swallow, { capture: true, once: true });
  setTimeout(() => window.removeEventListener("click", swallow, { capture: true }));
}

/**
 * Tracks a drag that starts with `down`, using window pointer events. Native HTML5 drags don't
 * work here: the Tauri drag-drop handler takes them over, so dragover and drop never fire.
 * Escape cancels. Returns a function that cancels the drag.
 */
export function trackPointerDrag(down: PointerEvent, callbacks: PointerDragCallbacks): () => void {
  if (down.button !== 0) return () => {};
  const origin = { x: down.clientX, y: down.clientY };
  let started = false;

  function move(event: PointerEvent) {
    if (!started) {
      if (!passedThreshold(origin, { x: event.clientX, y: event.clientY })) return;
      started = true;
      document.documentElement.classList.add("pointer-dragging");
      window.getSelection()?.removeAllRanges();
      callbacks.onStart?.(event);
    }
    callbacks.onMove(event);
  }

  function up(event: PointerEvent) {
    const wasStarted = started;
    stop();
    if (!wasStarted) return;
    swallowNextClick();
    callbacks.onDrop(event);
  }

  function cancel() {
    const wasStarted = started;
    stop();
    if (wasStarted) callbacks.onCancel();
  }

  function key(event: KeyboardEvent) {
    if (event.key === "Escape") cancel();
  }

  function stop() {
    started = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", key);
    document.documentElement.classList.remove("pointer-dragging");
  }

  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", key);
  return cancel;
}
