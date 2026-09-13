export class LruCache<K, V> {
  readonly #capacity: number;
  readonly #onEvict: ((key: K, value: V) => void) | undefined;
  readonly #items = new Map<K, V>();

  constructor(capacity: number, onEvict?: (key: K, value: V) => void) {
    this.#capacity = capacity;
    this.#onEvict = onEvict;
  }

  get size(): number {
    return this.#items.size;
  }

  get(key: K): V | undefined {
    if (!this.#items.has(key)) return undefined;
    const value = this.#items.get(key) as V;
    this.#items.delete(key);
    this.#items.set(key, value);
    return value;
  }

  set(key: K, value: V): void {
    this.#items.delete(key);
    this.#items.set(key, value);
    while (this.#items.size > this.#capacity) {
      const [oldestKey, oldestValue] = this.#items.entries().next().value as [K, V];
      this.#items.delete(oldestKey);
      this.#onEvict?.(oldestKey, oldestValue);
    }
  }
}
