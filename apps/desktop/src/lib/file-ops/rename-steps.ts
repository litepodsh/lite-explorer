/** Mirror of the rename steps in `src-tauri/src/explorer/rename/steps.rs`. */
export type Position = { kind: "start" } | { kind: "end" } | { kind: "index"; at: number };
export type Target = "name" | "extension" | "both";
export type CaseMode = "upper" | "lower" | "title" | "sentence";
export type Spaces = "keep" | "dash" | "underscore" | "remove";
export type DateSource = "created" | "modified" | "exif" | "today";

export type StepKind =
  | { type: "replace"; find: string; with: string; regex: boolean; caseSensitive: boolean; target: Target }
  | { type: "insert"; text: string; position: Position }
  | {
      type: "number";
      start: number;
      step: number;
      padding: number;
      separator: string;
      position: Position;
    }
  | { type: "case"; mode: CaseMode }
  | {
      type: "clean";
      trim: boolean;
      collapseSpaces: boolean;
      stripAccents: boolean;
      spaces: Spaces;
    }
  | { type: "date"; source: DateSource; format: string; separator: string; position: Position };

export type StepType = StepKind["type"];

/** A step in the dialog. `id` is only used to key the list; it never reaches the backend. */
export type RenameStep = StepKind & { id: string; enabled: boolean };

export const STEP_TYPES: readonly { type: StepType; label: string }[] = [
  { type: "replace", label: "Find & replace" },
  { type: "insert", label: "Add text" },
  { type: "number", label: "Number" },
  { type: "case", label: "Case" },
  { type: "clean", label: "Clean up" },
  { type: "date", label: "Date" },
];

export function stepLabel(type: StepType): string {
  return STEP_TYPES.find((entry) => entry.type === type)?.label ?? type;
}

let counter = 0;

function defaults(type: StepType): StepKind {
  switch (type) {
    case "replace":
      return { type, find: "", with: "", regex: false, caseSensitive: false, target: "name" };
    case "insert":
      return { type, text: "", position: { kind: "end" } };
    case "number":
      return { type, start: 1, step: 1, padding: 2, separator: " ", position: { kind: "end" } };
    case "case":
      return { type, mode: "title" };
    case "clean":
      return { type, trim: true, collapseSpaces: true, stripAccents: false, spaces: "keep" };
    case "date":
      return {
        type,
        source: "modified",
        format: "YYYY-MM-DD",
        separator: " ",
        position: { kind: "start" },
      };
  }
}

export function newStep(type: StepType): RenameStep {
  counter += 1;
  return { ...defaults(type), id: `step-${counter}`, enabled: true };
}

export function moveStep(steps: readonly RenameStep[], index: number, delta: -1 | 1): RenameStep[] {
  const target = index + delta;
  if (index < 0 || index >= steps.length || target < 0 || target >= steps.length) {
    return [...steps];
  }
  const next = [...steps];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

export function removeStep(steps: readonly RenameStep[], index: number): RenameStep[] {
  return steps.filter((_, position) => position !== index);
}

/** Number inputs give `null` when emptied; the backend wants plain integers. */
function whole(value: number | null | undefined, fallback = 0): number {
  return Number.isFinite(value) ? Math.trunc(value as number) : fallback;
}

function wirePosition(position: Position): Position {
  return position.kind === "index"
    ? { kind: "index", at: Math.max(0, whole(position.at)) }
    : { kind: position.kind };
}

/** The steps as the backend expects them: no ids, integers where integers are needed. */
export function toWire(steps: readonly RenameStep[]): Omit<RenameStep, "id">[] {
  return steps.map(({ id: _id, ...step }) => {
    switch (step.type) {
      case "insert":
        return { ...step, position: wirePosition(step.position) };
      case "number":
        return {
          ...step,
          start: whole(step.start),
          step: whole(step.step, 1),
          padding: Math.min(12, Math.max(0, whole(step.padding))),
          position: wirePosition(step.position),
        };
      case "date":
        return { ...step, position: wirePosition(step.position) };
      default:
        return step;
    }
  });
}

export type PlanStatus = "ok" | "unchanged" | "conflict" | "invalid";

export type PlannedItem = {
  path: string;
  oldName: string;
  newName: string;
  status: PlanStatus;
  reason: string | null;
};

export type RenamePlan = {
  items: PlannedItem[];
  stepErrors: { index: number; message: string }[];
};

export type PlanSummary = { change: number; unchanged: number; conflict: number; invalid: number };

export function summarize(items: readonly PlannedItem[]): PlanSummary {
  const summary: PlanSummary = { change: 0, unchanged: 0, conflict: 0, invalid: 0 };
  for (const item of items) {
    if (item.status === "ok") summary.change += 1;
    else summary[item.status] += 1;
  }
  return summary;
}

/** Whether the Rename button can be pressed: something changes and nothing is wrong. */
export function canApply(plan: RenamePlan | null): boolean {
  if (!plan || plan.stepErrors.length > 0) return false;
  const summary = summarize(plan.items);
  return summary.change > 0 && summary.conflict === 0 && summary.invalid === 0;
}

/** "3 will change · 2 unchanged · 1 conflict". */
export function summaryText(summary: PlanSummary): string {
  const parts = [`${summary.change} will change`];
  if (summary.unchanged > 0) parts.push(`${summary.unchanged} unchanged`);
  if (summary.conflict > 0) parts.push(`${summary.conflict} ${summary.conflict === 1 ? "conflict" : "conflicts"}`);
  if (summary.invalid > 0) parts.push(`${summary.invalid} invalid`);
  return parts.join(" · ");
}

export type NameDiff = { prefix: string; oldMiddle: string; newMiddle: string; suffix: string };

/** The part of the name that changed, found by trimming what both names share at each end. */
export function diffNames(oldName: string, newName: string): NameDiff {
  const oldChars = [...oldName];
  const newChars = [...newName];
  let start = 0;
  while (start < oldChars.length && start < newChars.length && oldChars[start] === newChars[start]) {
    start += 1;
  }
  let end = 0;
  while (
    end < oldChars.length - start &&
    end < newChars.length - start &&
    oldChars[oldChars.length - 1 - end] === newChars[newChars.length - 1 - end]
  ) {
    end += 1;
  }
  return {
    prefix: oldChars.slice(0, start).join(""),
    oldMiddle: oldChars.slice(start, oldChars.length - end).join(""),
    newMiddle: newChars.slice(start, newChars.length - end).join(""),
    suffix: oldChars.slice(oldChars.length - end).join(""),
  };
}
