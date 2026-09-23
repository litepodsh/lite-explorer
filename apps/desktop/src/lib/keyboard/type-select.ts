/** Pause after which typed letters start a new query, as in Finder and Explorer. */
export const TYPE_SELECT_TIMEOUT_MS = 1000;

/** Case- and accent-insensitive form for prefix matching. */
function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLocaleLowerCase();
}

/** Collects letters typed in quick succession into one query. */
export class TypeSelectBuffer {
  #query = "";
  #lastAt = 0;

  constructor(private readonly timeoutMs = TYPE_SELECT_TIMEOUT_MS) {}

  /** Adds `char` typed at `now` and returns the query to match. */
  push(char: string, now: number): string {
    if (now - this.#lastAt > this.timeoutMs) this.#query = "";
    this.#lastAt = now;
    this.#query += char;
    return this.#query;
  }

  reset() {
    this.#query = "";
    this.#lastAt = 0;
  }
}

/**
 * Index of the entry type-to-select jumps to, or null when nothing matches.
 * The first name starting with `query` wins. Repeating one letter (`r`, `rr`, …)
 * cycles through the names starting with it, after `current`.
 */
export function typeSelectTarget(names: string[], query: string, current: number): number | null {
  if (!query) return null;
  const folded = names.map(fold);
  const needle = fold(query);
  const letter = needle[0];
  const repeated = needle.length > 1 && [...needle].every((char) => char === letter);
  if (!repeated) {
    const first = folded.findIndex((name) => name.startsWith(needle));
    return first === -1 ? null : first;
  }
  for (let step = 1; step <= folded.length; step++) {
    const index = (current + step) % folded.length;
    if (folded[index].startsWith(letter)) return index;
  }
  return null;
}
