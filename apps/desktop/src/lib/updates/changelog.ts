import "temporal-polyfill/global";
import generated from "./changelog.generated.json";
import type { ReleaseNotes } from "./parse-changelog.js";

export type { ChangeGroup, ChangeKind, ReleaseNotes } from "./parse-changelog.js";

export const releases = generated.releases as ReleaseNotes[];

export const latestVersion = releases[0]?.version ?? null;

export function releaseFor(version: string | null): ReleaseNotes | null {
  if (!version) return null;
  return releases.find((release) => release.version === version) ?? null;
}

export function formatDate(date: string | null): string {
  if (!date) return "";
  try {
    return Temporal.PlainDate.from(date).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return date;
  }
}

export function noteLines(notes: string | null): string[] {
  if (!notes) return [];
  return notes
    .split(/\r?\n/)
    .map((line) => line.replace(/^\s*[-*]\s+/, "").trim())
    .filter((line) => line.length > 0);
}
