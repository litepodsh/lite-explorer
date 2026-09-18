import { describe, expect, test } from "bun:test";
import { Typewriter } from "./typewriter.js";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe("Typewriter", () => {
  test("writes the text one piece at a time and finishes with the whole string", async () => {
    const written: string[] = [];
    const typewriter = new Typewriter();
    const done = typewriter.start("Quarterly Report.pdf", (value) => written.push(value));
    expect(typewriter.running).toBe(true);
    await done;
    expect(typewriter.running).toBe(false);
    expect(written.at(-1)).toBe("Quarterly Report.pdf");
    expect(written.length > 1).toBe(true);
  });

  test("a cancel mid-write stops the text from growing", async () => {
    const written: string[] = [];
    const typewriter = new Typewriter();
    void typewriter.start("a much longer suggested name.txt", (value) => written.push(value));
    await wait(40);
    typewriter.cancel();
    const length = written.length;
    await wait(80);
    expect(written.length).toBe(length);
    expect(typewriter.running).toBe(false);
  });

  test("a second start replaces the first instead of interleaving", async () => {
    const written: string[] = [];
    const typewriter = new Typewriter();
    void typewriter.start("first name.txt", (value) => written.push(value));
    await typewriter.start("second.md", (value) => written.push(value));
    await wait(60);
    expect(written.at(-1)).toBe("second.md");
    expect(written.some((value) => value.startsWith("first") && value.length > 6)).toBe(false);
  });

  test("empty text writes nothing", async () => {
    const written: string[] = [];
    await new Typewriter().start("", (value) => written.push(value));
    expect(written).toEqual([]);
  });
});
