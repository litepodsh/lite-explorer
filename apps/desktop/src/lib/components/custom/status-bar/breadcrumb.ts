export type BreadcrumbSegment = { label: string; path: string };

type Token = { name: string; path: string };

const isSeparator = (character: string | undefined) => character === "/" || character === "\\";

/** Drops a trailing separator, keeping `/` and a bare `scheme://`. */
function trimSeparator(path: string): string {
  return path.length > 1 && isSeparator(path.at(-1)) && !path.endsWith("://") ? path.slice(0, -1) : path;
}

/** Splits a local, Windows, UNC or `scheme://host` path into folders, each with the path that opens it.
 *  A scheme and its host, a UNC server or `/` form the first folder. */
function tokenize(path: string): Token[] {
  if (!path) return [];
  const root =
    path.match(/^[a-z][a-z0-9+.-]*:\/\/[^/]*\/?/i)?.[0] ??
    path.match(/^\\\\[^\\/]+[\\/]?/)?.[0] ??
    (isSeparator(path[0]) ? path[0] : "");
  const tokens: Token[] = [];
  if (root) {
    const rootPath = isSeparator(root.at(-1)) ? root : `${root}/`;
    tokens.push({ name: trimSeparator(root), path: rootPath });
  }
  for (const match of path.slice(root.length).matchAll(/[^\\/]+/g)) {
    const end = root.length + match.index + match[0].length;
    const separator = isSeparator(path[end]) ? path[end] : "";
    tokens.push({ name: match[0], path: path.slice(0, end) + separator });
  }
  return tokens;
}

/** Folders of `internal` labelled as `display` shows them. The folders both paths end with are paired;
 *  what is left of the display path (like `s3://bucket` or `smb://server/share`) labels the folder
 *  where the internal path's own root ends, so every segment opens a real internal path. */
export function breadcrumbSegments(internal: string, display: string): BreadcrumbSegment[] {
  const inner = tokenize(internal);
  const shown = tokenize(display || internal);
  let shared = 0;
  while (
    shared < inner.length &&
    shared < shown.length &&
    inner[inner.length - 1 - shared].name === shown[shown.length - 1 - shared].name
  )
    shared++;
  const segments = inner.slice(inner.length - shared).map(({ name, path }) => ({ label: name, path }));
  const innerRoot = inner[inner.length - shared - 1];
  const shownRoot = shown[shown.length - shared - 1];
  if (innerRoot && shownRoot) segments.unshift({ label: trimSeparator(shownRoot.path), path: innerRoot.path });
  return segments.length ? segments : inner.map(({ name, path }) => ({ label: name, path }));
}

const MAX_VISIBLE = 4;
const TAIL = 3;

/** Keeps the root and the last folders of a long path visible; the rest go behind an ellipsis. */
export function collapseSegments(segments: BreadcrumbSegment[]) {
  if (segments.length <= MAX_VISIBLE) return { head: segments, hidden: [], tail: [] };
  return {
    head: segments.slice(0, 1),
    hidden: segments.slice(1, -TAIL),
    tail: segments.slice(-TAIL),
  };
}
