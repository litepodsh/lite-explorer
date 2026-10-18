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
  // Plain `YYYY-MM-DD` only. Date parses it as UTC midnight, so it is formatted in UTC to keep the day.
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return date;
  return parsed.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });
}

export function noteLines(notes: string | null): string[] {
  if (!notes) return [];
  return notes
    .split(/\r?\n/)
    .map((line) => line.replace(/^\s*[-*]\s+/, "").trim())
    .filter((line) => line.length > 0);
}
