import type { CommandRegistry, CommandResult } from "./commands.js";
import { eventToken, type KeyPlatform } from "./keys.js";
import type { Binding, Mode } from "./keymap.js";
import { buildIndex, resolve, type ChordState, type KeymapIndex, type ResolvedBinding } from "./resolve.js";
import { readScopeFacts, scopeFromFacts, type Scope } from "./scope.js";

export type DispatcherOptions<C> = {
  registry: CommandRegistry<C>;
  bindings: () => Binding[];
  platform: () => KeyPlatform;
  mode: () => Mode;
  timeoutMs: () => number;
  /** A confirmation or context menu is open even if focus is elsewhere. */
  modalOpen: () => boolean;
  context: (scope: Scope) => C;
  onError: (title: string, error: unknown) => void;
};

/**
 * Runs keymap commands from `keydown` on the window, in the bubble phase: handlers on elements
 * (rename fields, find bars, Monaco) run first and win with `preventDefault`.
 */
export class KeyboardDispatcher<C> {
  pending = $state.raw<ChordState | null>(null);
  #index: KeymapIndex | null = null;
  #indexBindings: Binding[] | null = null;
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly options: DispatcherOptions<C>) {}

  cancel() {
    clearTimeout(this.#timer);
    this.pending = null;
  }

  handle = (event: KeyboardEvent) => {
    if (event.defaultPrevented) return;
    const token = eventToken(event);
    if (!token) return;
    const scope = scopeFromFacts(readScopeFacts(event.target, this.options.modalOpen()));
    const resolution = resolve({
      index: this.#currentIndex(),
      scope,
      mode: this.options.mode(),
      token,
      repeat: event.repeat,
      chord: this.pending,
      now: Date.now(),
      timeoutMs: this.options.timeoutMs(),
    });
    switch (resolution.kind) {
      case "none":
        this.cancel();
        return;
      case "cancel":
        event.preventDefault();
        this.cancel();
        return;
      case "pending":
        event.preventDefault();
        this.#setPending(resolution.chord);
        return;
      case "run":
        this.cancel();
        this.#run(resolution.binding, resolution.args, scope, event);
    }
  };

  #currentIndex(): KeymapIndex {
    const platform = this.options.platform();
    const bindings = this.options.bindings();
    if (!this.#index || this.#index.platform !== platform || this.#indexBindings !== bindings) {
      this.#index = buildIndex(bindings, platform);
      this.#indexBindings = bindings;
    }
    return this.#index;
  }

  #setPending(chord: ChordState) {
    clearTimeout(this.#timer);
    this.pending = chord;
    this.#timer = setTimeout(() => this.cancel(), this.options.timeoutMs());
  }

  #run(binding: ResolvedBinding, args: Record<string, unknown>, scope: Scope, event: KeyboardEvent) {
    const command = binding.command ? this.options.registry.get(binding.command) : undefined;
    if (!command) return;
    const context = this.options.context(scope);
    if (command.when && !command.when(context)) return;
    let result: CommandResult;
    try {
      result = command.run(context, args);
    } catch (error) {
      event.preventDefault();
      this.options.onError(command.title, error);
      return;
    }
    if (result === false) return;
    event.preventDefault();
    if (result instanceof Promise) result.catch((error: unknown) => this.options.onError(command.title, error));
  }
}
