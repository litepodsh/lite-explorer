import { describe, expect, test } from "bun:test";
import { HoldGesture } from "./hold-gesture.js";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe("HoldGesture", () => {
  test("releasing early does not confirm", async () => {
    let confirmed = 0;
    const gesture = new HoldGesture(40, () => confirmed++);
    gesture.start();
    await wait(15);
    gesture.cancel();
    await wait(50);
    expect(confirmed).toBe(0);
    expect(gesture.holding).toBe(false);
  });

  test("holding for the full duration confirms once", async () => {
    let confirmed = 0;
    const states: string[] = [];
    const gesture = new HoldGesture(
      30,
      () => confirmed++,
      (s) => states.push(`${s.holding}/${s.confirmed}`),
    );
    gesture.start();
    gesture.start();
    await wait(60);
    gesture.start();
    await wait(60);
    expect(confirmed).toBe(1);
    expect(gesture.confirmed).toBe(true);
    expect(states).toEqual(["true/false", "false/true"]);
  });
});
