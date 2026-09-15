/** `false` means the key did nothing, so the dispatcher leaves its default action alone. */
export type CommandResult = boolean | void | Promise<void>;

export type Command<C> = {
  id: string;
  title: string;
  when?: (context: C) => boolean;
  run: (context: C, args: Record<string, unknown>) => CommandResult;
};

export class CommandRegistry<C> {
  #commands = new Map<string, Command<C>>();

  /** Returns a function that unregisters these commands. */
  register(commands: Command<C>[]): () => void {
    for (const command of commands) {
      if (this.#commands.has(command.id)) throw new Error(`Command “${command.id}” is already registered`);
      this.#commands.set(command.id, command);
    }
    return () => {
      for (const command of commands) {
        if (this.#commands.get(command.id) === command) this.#commands.delete(command.id);
      }
    };
  }

  get(id: string): Command<C> | undefined {
    return this.#commands.get(id);
  }

  has(id: string): boolean {
    return this.#commands.has(id);
  }

  list(): Command<C>[] {
    return [...this.#commands.values()];
  }

  available(id: string, context: C): boolean {
    const command = this.#commands.get(id);
    return Boolean(command) && (command?.when?.(context) ?? true);
  }

  run(id: string, context: C, args: Record<string, unknown> = {}): CommandResult {
    const command = this.#commands.get(id);
    if (!command || !(command.when?.(context) ?? true)) return false;
    return command.run(context, args);
  }
}
