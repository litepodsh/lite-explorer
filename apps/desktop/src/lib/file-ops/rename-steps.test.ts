import { describe, expect, test } from "bun:test";
import {
  STEP_TYPES,
  canApply,
  diffNames,
  moveStep,
  newStep,
  removeStep,
  stepLabel,
  summarize,
  summaryText,
  toWire,
  type PlannedItem,
  type RenamePlan,
} from "./rename-steps.js";

const item = (status: PlannedItem["status"], oldName = "a", newName = "b"): PlannedItem => ({
  path: `/x/${oldName}`,
  oldName,
  newName,
  status,
  reason: null,
});

describe("newStep", () => {
  test("every type starts enabled with a unique id", () => {
    const steps = STEP_TYPES.map((entry) => newStep(entry.type));
    expect(steps.every((step) => step.enabled)).toBe(true);
    expect(new Set(steps.map((step) => step.id)).size).toBe(steps.length);
    expect(steps.map((step) => step.type)).toEqual(STEP_TYPES.map((entry) => entry.type));
  });

  test("defaults are harmless or sensible", () => {
    const replace = newStep("replace");
    expect(replace.type === "replace" && replace.find === "").toBe(true);
    const number = newStep("number");
    expect(number.type === "number" && number.start === 1 && number.padding === 2).toBe(true);
    const date = newStep("date");
    expect(date.type === "date" && date.format === "YYYY-MM-DD").toBe(true);
  });

  test("labels", () => {
    expect(stepLabel("replace")).toBe("Find & replace");
    expect(stepLabel("clean")).toBe("Clean up");
  });
});

describe("list editing", () => {
  const a = newStep("replace");
  const b = newStep("insert");
  const c = newStep("case");

  test("moveStep swaps neighbours and ignores the edges", () => {
    expect(moveStep([a, b, c], 1, -1).map((step) => step.id)).toEqual([b.id, a.id, c.id]);
    expect(moveStep([a, b, c], 1, 1).map((step) => step.id)).toEqual([a.id, c.id, b.id]);
    expect(moveStep([a, b, c], 0, -1).map((step) => step.id)).toEqual([a.id, b.id, c.id]);
    expect(moveStep([a, b, c], 2, 1).map((step) => step.id)).toEqual([a.id, b.id, c.id]);
  });

  test("removeStep drops one step", () => {
    expect(removeStep([a, b, c], 1).map((step) => step.id)).toEqual([a.id, c.id]);
  });

  test("editing helpers never mutate their input", () => {
    const original = [a, b, c];
    moveStep(original, 0, 1);
    removeStep(original, 0);
    expect(original.map((step) => step.id)).toEqual([a.id, b.id, c.id]);
  });
});

describe("toWire", () => {
  test("drops ids and keeps the rest", () => {
    const [wire] = toWire([newStep("case")]);
    expect("id" in wire).toBe(false);
    expect(wire).toEqual({ type: "case", mode: "title", enabled: true });
  });

  test("cleans up number fields left empty or fractional", () => {
    const step = newStep("number");
    if (step.type !== "number") throw new Error("expected a number step");
    step.start = null as unknown as number;
    step.step = 2.9;
    step.padding = 99;
    step.position = { kind: "index", at: -3 };
    const [wire] = toWire([step]);
    expect(wire).toMatchObject({
      start: 0,
      step: 2,
      padding: 12,
      position: { kind: "index", at: 0 },
    });
  });

  test("positions lose stray fields", () => {
    const step = newStep("insert");
    if (step.type !== "insert") throw new Error("expected an insert step");
    step.position = { kind: "start", at: 5 } as never;
    const [wire] = toWire([step]);
    expect(wire).toMatchObject({ position: { kind: "start" } });
    expect("at" in (wire as unknown as { position: object }).position).toBe(false);
  });
});

describe("summary", () => {
  test("counts each status", () => {
    const summary = summarize([item("ok"), item("ok"), item("unchanged"), item("conflict"), item("invalid")]);
    expect(summary).toEqual({ change: 2, unchanged: 1, conflict: 1, invalid: 1 });
  });

  test("summaryText mentions only what matters", () => {
    expect(summaryText({ change: 3, unchanged: 0, conflict: 0, invalid: 0 })).toBe("3 will change");
    expect(summaryText({ change: 3, unchanged: 2, conflict: 1, invalid: 0 })).toBe(
      "3 will change · 2 unchanged · 1 conflict",
    );
    expect(summaryText({ change: 0, unchanged: 1, conflict: 2, invalid: 1 })).toBe(
      "0 will change · 1 unchanged · 2 conflicts · 1 invalid",
    );
  });
});

describe("canApply", () => {
  const plan = (items: PlannedItem[], stepErrors: RenamePlan["stepErrors"] = []): RenamePlan => ({
    items,
    stepErrors,
  });

  test("needs a plan with at least one change and nothing wrong", () => {
    expect(canApply(null)).toBe(false);
    expect(canApply(plan([item("ok"), item("unchanged")]))).toBe(true);
    expect(canApply(plan([item("unchanged")]))).toBe(false);
    expect(canApply(plan([]))).toBe(false);
  });

  test("conflicts, invalid names and broken steps block it", () => {
    expect(canApply(plan([item("ok"), item("conflict")]))).toBe(false);
    expect(canApply(plan([item("ok"), item("invalid")]))).toBe(false);
    expect(canApply(plan([item("ok")], [{ index: 0, message: "Invalid pattern" }]))).toBe(false);
  });
});

describe("diffNames", () => {
  test("finds the changed middle", () => {
    expect(diffNames("IMG_001.jpg", "Trip 001.jpg")).toEqual({
      prefix: "",
      oldMiddle: "IMG_",
      newMiddle: "Trip ",
      suffix: "001.jpg",
    });
    expect(diffNames("photo.png", "photo 2.png")).toEqual({
      prefix: "photo",
      oldMiddle: "",
      newMiddle: " 2",
      suffix: ".png",
    });
  });

  test("identical and fully different names", () => {
    expect(diffNames("a", "a")).toEqual({ prefix: "a", oldMiddle: "", newMiddle: "", suffix: "" });
    expect(diffNames("abc", "xyz")).toEqual({ prefix: "", oldMiddle: "abc", newMiddle: "xyz", suffix: "" });
  });

  test("works on characters, not bytes", () => {
    expect(diffNames("añb", "aéb")).toEqual({ prefix: "a", oldMiddle: "ñ", newMiddle: "é", suffix: "b" });
  });
});
