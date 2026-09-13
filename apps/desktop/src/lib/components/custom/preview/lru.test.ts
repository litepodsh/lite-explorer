import { describe, expect, test } from "bun:test";
import { LruCache } from "./lru.js";

describe("LruCache", () => {
  test("evicts the least recently used entry", () => {
    const evicted: Array<[string, number]> = [];
    const cache = new LruCache<string, number>(2, (key, value) => evicted.push([key, value]));

    cache.set("a", 1);
    cache.set("b", 2);
    expect(cache.get("a")).toBe(1);
    cache.set("c", 3);

    expect(evicted).toEqual([["b", 2]]);
    expect(cache.get("b")).toBeUndefined();
    expect(cache.get("a")).toBe(1);
    expect(cache.get("c")).toBe(3);
    expect(cache.size).toBe(2);
  });

  test("updating an existing key does not evict", () => {
    const evicted: string[] = [];
    const cache = new LruCache<string, number>(2, (key) => evicted.push(key));

    cache.set("a", 1);
    cache.set("b", 2);
    cache.set("a", 10);
    cache.set("c", 3);

    expect(cache.get("a")).toBe(10);
    expect(evicted).toEqual(["b"]);
  });
});
