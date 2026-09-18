export type Region = "list" | "sidebar" | "preview";
export type Scope = "global" | Region | "dialog" | "input" | "monaco";

export type ScopeFacts = {
  dialog: boolean;
  monaco: boolean;
  input: boolean;
  /** A button or link that is not a file entry. */
  control: boolean;
  region: Region | null;
};

const REGIONS: readonly Region[] = ["list", "sidebar", "preview"];

export function scopeFromFacts(facts: ScopeFacts): Scope {
  if (facts.dialog) return "dialog";
  if (facts.monaco) return "monaco";
  if (facts.input) return "input";
  if (facts.region) return facts.region;
  if (facts.control) return "global";
  return "list";
}

/** Reads scope facts from the element a key event targets. Browser only. */
export function readScopeFacts(target: EventTarget | null, modalOpen: boolean): ScopeFacts {
  const element = target instanceof Element ? target : null;
  const marked = element?.closest("[data-key-scope]")?.getAttribute("data-key-scope");
  return {
    dialog:
      modalOpen ||
      Boolean(element?.closest("[role='dialog'], [role='alertdialog'], [role='menu']")),
    monaco: Boolean(element?.closest(".monaco-editor")),
    input: Boolean(element?.closest("input, textarea, select, [contenteditable='true']")),
    control: Boolean(element?.closest("button, a")) && !element?.closest("[data-entry-path]"),
    region: REGIONS.find((region) => region === marked) ?? null,
  };
}
