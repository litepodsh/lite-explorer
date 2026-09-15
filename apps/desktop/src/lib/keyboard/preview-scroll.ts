const STEP_PX = 80;

/** Scrolls preview content by about five lines. Monaco scrolls itself from wheel events. */
export function scrollPreview(container: HTMLElement, direction: 1 | -1): boolean {
  const deltaY = direction * STEP_PX;
  const monaco = container.querySelector<HTMLElement>(".monaco-scrollable-element");
  if (monaco) {
    monaco.dispatchEvent(new WheelEvent("wheel", { deltaY, bubbles: true, cancelable: true }));
    return true;
  }
  const candidates = [container, ...container.querySelectorAll<HTMLElement>("*")];
  const scrollable = candidates.find(
    (element) => element.scrollHeight > element.clientHeight + 1 && /(auto|scroll)/.test(getComputedStyle(element).overflowY),
  );
  if (!scrollable) return false;
  scrollable.scrollBy({ top: deltaY });
  return true;
}
