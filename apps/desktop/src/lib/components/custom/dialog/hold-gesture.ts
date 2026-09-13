const pressedKeys = new Set<string>();

if (typeof window !== "undefined") {
  window.addEventListener("keydown", (event) => pressedKeys.add(event.key));
  window.addEventListener("keyup", (event) => pressedKeys.delete(event.key));
  window.addEventListener("blur", () => pressedKeys.clear());
}

/** Whether `key` is physically held right now, regardless of focus. */
export function isKeyPressed(key: string): boolean {
  return pressedKeys.has(key);
}

/** Press-and-hold confirmation: `onconfirm` fires only after the press lasts `holdMs`. */
export class HoldGesture {
  #timer: ReturnType<typeof setTimeout> | null = null;
  #confirmed = false;

  constructor(
    private readonly holdMs: number,
    private readonly onconfirm: () => void,
    private readonly onchange: (state: { holding: boolean; confirmed: boolean }) => void = () => {},
  ) {}

  get holding(): boolean {
    return this.#timer !== null;
  }

  get confirmed(): boolean {
    return this.#confirmed;
  }

  start(): void {
    if (this.#confirmed || this.#timer) return;
    this.#timer = setTimeout(() => {
      this.#timer = null;
      this.#confirmed = true;
      this.onchange({ holding: false, confirmed: true });
      this.onconfirm();
    }, this.holdMs);
    this.onchange({ holding: true, confirmed: false });
  }

  cancel(): void {
    if (!this.#timer) return;
    clearTimeout(this.#timer);
    this.#timer = null;
    this.onchange({ holding: false, confirmed: this.#confirmed });
  }
}
