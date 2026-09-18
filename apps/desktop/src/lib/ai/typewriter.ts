/** How long each character takes to appear when the suggestion is typed out. */
const CHAR_MS = 22;
/** Long names would take seconds one character at a time, so the pace speeds up with length. */
const MAX_TOTAL_MS = 900;

/**
 * Writes text into a field one character at a time.
 *
 * The model answers in one piece; the animation is what makes the suggestion feel like it is being
 * written rather than pasted. Honors `prefers-reduced-motion` by writing it all at once.
 */
export class Typewriter {
  #timer: ReturnType<typeof setTimeout> | null = null;

  get running(): boolean {
    return this.#timer !== null;
  }

  /** Writes `text` through `write`, replacing whatever it wrote before. */
  start(text: string, write: (value: string) => void): Promise<void> {
    this.cancel();
    if (!text) return Promise.resolve();
    if (prefersReducedMotion()) {
      write(text);
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      const step = Math.max(1, Math.round(text.length / (MAX_TOTAL_MS / CHAR_MS)));
      let index = 0;
      const tick = () => {
        index = Math.min(text.length, index + step);
        write(text.slice(0, index));
        if (index >= text.length) {
          this.#timer = null;
          resolve();
          return;
        }
        this.#timer = setTimeout(tick, CHAR_MS);
      };
      this.#timer = setTimeout(tick, CHAR_MS);
    });
  }

  cancel() {
    if (this.#timer === null) return;
    clearTimeout(this.#timer);
    this.#timer = null;
  }
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}
