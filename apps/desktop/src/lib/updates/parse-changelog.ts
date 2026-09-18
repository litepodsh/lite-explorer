export type ChangeKind = "added" | "changed" | "deprecated" | "removed" | "fixed" | "security";

export type ChangeGroup = { kind: ChangeKind; label: string; items: string[] };

export type ReleaseNotes = {
  version: string;
  date: string | null;
  groups: ChangeGroup[];
};

const GROUPS: { kind: ChangeKind; label: string }[] = [
  { kind: "added", label: "Added" },
  { kind: "changed", label: "Changed" },
  { kind: "deprecated", label: "Deprecated" },
  { kind: "removed", label: "Removed" },
  { kind: "fixed", label: "Fixed" },
  { kind: "security", label: "Security" },
];

function groupOf(heading: string): { kind: ChangeKind; label: string } | null {
  const label = heading.trim().toLowerCase();
  return GROUPS.find((group) => group.label.toLowerCase() === label) ?? null;
}

function headingLevel(line: string): number {
  const match = /^(#+)\s+/.exec(line);
  return match ? match[1].length : 0;
}

function parseReleaseHeading(line: string): { version: string; date: string | null } {
  const title = line.replace(/^##\s+/, "").trim();
  const [raw, ...rest] = title.split(/\s+-\s+/);
  const version = raw.replace(/^\[/, "").replace(/\]$/, "").trim();
  const date = rest.join(" - ").trim();
  return { version, date: /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : null };
}

export function parseChangelog(source: string): ReleaseNotes[] {
  const releases: ReleaseNotes[] = [];
  let current: ReleaseNotes | null = null;
  let group: ChangeGroup | null = null;

  for (const raw of source.split(/\r?\n/)) {
    const line = raw.trimEnd();
    const level = headingLevel(line);
    if (level === 2) {
      current = { ...parseReleaseHeading(line), groups: [] };
      group = null;
      releases.push(current);
      continue;
    }
    if (level === 3 && current) {
      const kind = groupOf(line.replace(/^###\s+/, ""));
      group = kind ? { ...kind, items: [] } : null;
      if (group) current.groups.push(group);
      continue;
    }
    if (!current) continue;
    const bullet = /^\s*[-*]\s+(.+)$/.exec(line);
    if (!bullet) continue;
    if (!group) {
      group = { kind: "changed", label: "Changed", items: [] };
      current.groups.push(group);
    }
    group.items.push(bullet[1].trim());
  }

  return releases.filter((release) => release.groups.length > 0);
}
