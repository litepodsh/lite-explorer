declare module "bun:test" {
  export function describe(name: string, run: () => void): void;
  export function test(name: string, run: () => void | Promise<void>): void;
  export function expect(actual: unknown): {
    toBe(expected: unknown): void;
    toEqual(expected: unknown): void;
    toMatchObject(expected: unknown): void;
    toBeUndefined(): void;
    toBeNull(): void;
  };
}
