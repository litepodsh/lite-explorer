import { appliesTo, type Binding, type Mode } from "./keymap.js";
import { joinToken, normalizeToken, type KeyPlatform, type Modifier } from "./keys.js";
import type { Scope } from "./scope.js";

export type ResolvedBinding = Binding & { tokens: string[] };
export type KeymapIndex = { platform: KeyPlatform; byFirst: Map<string, ResolvedBinding[]> };
export type ChordState = { typed: string[]; candidates: ResolvedBinding[]; startedAt: number };

export type Resolution =
  | { kind: "run"; binding: ResolvedBinding; args: Record<string, unknown> }
  | { kind: "pending"; chord: ChordState }
  | { kind: "cancel" }
  | { kind: "none" };

export type ResolveInput = {
  index: KeymapIndex;
  scope: Scope;
  mode: Mode;
  token: string;
  repeat: boolean;
  chord: ChordState | null;
  now: number;
  timeoutMs: number;
};

export function buildIndex(bindings: Binding[], platform: KeyPlatform): KeymapIndex {
  const byFirst = new Map<string, ResolvedBinding[]>();
  for (const binding of bindings) {
    if (!appliesTo(binding, platform)) continue;
    const tokens = binding.keys.map((key) => normalizeToken(key, platform));
    const list = byFirst.get(tokens[0]) ?? [];
    list.push({ ...binding, tokens });
    byFirst.set(tokens[0], list);
  }
  return { platform, byFirst };
}

function allowedIn(binding: Binding, scope: Scope): boolean {
  if (binding.source === "menu" || binding.source === "component") return false;
  if (scope === "dialog" || scope === "input" || scope === "monaco") {
    return binding.scope === scope || (binding.scope === "global" && Boolean(binding.where?.includes(scope)));
  }
  return binding.scope === scope || binding.scope === "global";
}

function modeRank(binding: Binding, mode: Mode): number | null {
  if (binding.mode === mode) return 0;
  if (mode === "yazi" && binding.mode === "standard") return 1;
  return null;
}

function candidates(index: KeymapIndex, token: string, scope: Scope, mode: Mode): ResolvedBinding[] {
  const matches = (index.byFirst.get(token) ?? []).filter(
    (binding) => allowedIn(binding, scope) && modeRank(binding, mode) !== null,
  );
  const overridden = new Set(
    matches
      .filter((binding) => mode === "yazi" && binding.mode === "yazi" && binding.overrides)
      .map((binding) => `${binding.scope} ${binding.tokens.join(" ")}`),
  );
  return matches
    .filter((binding) => !(binding.mode === "standard" && overridden.has(`${binding.scope} ${binding.tokens.join(" ")}`)))
    .sort(
      (a, b) =>
        (a.scope === scope ? 0 : 1) - (b.scope === scope ? 0 : 1) ||
        (modeRank(a, mode) ?? 0) - (modeRank(b, mode) ?? 0),
    );
}

/** Removes Shift and the primary modifier from a token, reporting which were removed. */
function stripModifiers(token: string, platform: KeyPlatform): { base: string; removed: ("Shift" | "Mod")[] } | null {
  if (token.length === 1) return null;
  const parts = token.split("+");
  const key = parts.pop() ?? "";
  const primary: Modifier = platform === "macos" ? "Meta" : "Ctrl";
  const removed: ("Shift" | "Mod")[] = [];
  const kept = (parts as Modifier[]).filter((modifier) => {
    if (modifier === "Shift") {
      removed.push("Shift");
      return false;
    }
    if (modifier === primary) {
      removed.push("Mod");
      return false;
    }
    return true;
  });
  return removed.length ? { base: joinToken(kept, key), removed } : null;
}

function runOrSkip(binding: ResolvedBinding, repeat: boolean, args: Record<string, unknown>): Resolution {
  return repeat && binding.repeat === false ? { kind: "none" } : { kind: "run", binding, args };
}

export function resolve(input: ResolveInput): Resolution {
  const { index, scope, mode, token, chord } = input;

  if (chord && input.now - chord.startedAt <= input.timeoutMs) {
    if (token === "<Esc>") return { kind: "cancel" };
    const typed = [...chord.typed, token];
    const remaining = chord.candidates.filter((binding) => binding.tokens[typed.length - 1] === token);
    const complete = remaining.find((binding) => binding.tokens.length === typed.length);
    if (complete) return { kind: "run", binding: complete, args: complete.args ?? {} };
    if (remaining.length === 0) return { kind: "cancel" };
    return { kind: "pending", chord: { typed, candidates: remaining, startedAt: chord.startedAt } };
  }

  const exact = candidates(index, token, scope, mode);
  if (exact.length > 0) {
    const single = exact.find((binding) => binding.tokens.length === 1);
    if (single) return runOrSkip(single, input.repeat, single.args ?? {});
    if (input.repeat) return { kind: "none" };
    return { kind: "pending", chord: { typed: [token], candidates: exact, startedAt: input.now } };
  }

  const stripped = stripModifiers(token, index.platform);
  if (!stripped) return { kind: "none" };
  const match = candidates(index, stripped.base, scope, mode).find(
    (binding) => binding.tokens.length === 1 && stripped.removed.every((modifier) => binding.modifiers?.includes(modifier)),
  );
  if (!match) return { kind: "none" };
  return runOrSkip(match, input.repeat, {
    ...match.args,
    shift: stripped.removed.includes("Shift"),
    primary: stripped.removed.includes("Mod"),
  });
}
