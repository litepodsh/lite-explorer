// Minimal ambient typings for `bun:test` so svelte-check can type-check the
// test suite. The real types live in `bun-types` (via `@types/bun`), but the
// generated SvelteKit tsconfig does not pick them up, so this file keeps the
// editor/CI check green. Extend the matchers here if a new test needs more.

type Matcher<T> = {
  toBe(expected: unknown): void;
  toEqual(expected: unknown): void;
  toMatchObject(expected: unknown): void;
  toBeUndefined(): void;
  toBeNull(): void;
  toBeGreaterThan(expected: number): void;
  toContain(expected: unknown): void;
  toMatch(expected: string | RegExp): void;
};

declare module "bun:test" {
  export function describe(name: string, run: () => void): void;
  export function test(name: string, run: () => void | Promise<void>): void;
  export function it(name: string, run: () => void | Promise<void>): void;
  export function beforeEach(run: () => void | Promise<void>): void;
  export function afterEach(run: () => void | Promise<void>): void;
  export function expect(actual: unknown): Matcher<unknown> & {
    toHaveBeenCalledTimes(count: number): void;
    toHaveBeenCalledWith(...args: unknown[]): void;
  };

  export interface Mock<T extends (...args: never[]) => unknown> {
    (...args: Parameters<T>): ReturnType<T>;
    mock: { calls: Array<{ args: unknown[] }> };
    mockReset(): void;
    mockResolvedValue(value: Awaited<ReturnType<T>>): void;
    mockRejectedValue(value: unknown): void;
    mockImplementation(implementation: T): void;
  }

  export function mock<T extends (...args: never[]) => unknown>(implementation: T): Mock<T>;

  export function mock<const T extends (...args: never[]) => unknown = () => never>(
    implementation?: T,
  ): Mock<T>;

  export namespace mock {
    function module(specifier: string, factory: () => Record<string, unknown>): void;
  }
}

declare module "bun" {
  export * from "bun:test";
}
