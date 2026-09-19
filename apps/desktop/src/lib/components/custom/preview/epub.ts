import { invoke } from "@tauri-apps/api/core";

export type EpubTocEntry = {
  label: string;
  href: string | null;
  depth: number;
};

export type EpubChapter = {
  href: string;
  title: string;
};

export type EpubDocument = {
  title: string;
  author: string | null;
  language: string | null;
  /** `media://` base; an inner package path is appended as `/<path>`. */
  base: string;
  cover: string | null;
  styles: string[];
  chapters: EpubChapter[];
  toc: EpubTocEntry[];
};

/** Registers the package and returns its metadata, spine and TOC. */
export function openEpub(path: string): Promise<EpubDocument> {
  return invoke<EpubDocument>("open_epub", { path });
}

/** Raw chapter XHTML (scripts already stripped by the backend). */
export function readEpubChapter(path: string, href: string): Promise<string> {
  return invoke<string>("read_epub_chapter", { path, href });
}

/** Absolute `media://` URL for a package-inner path. */
export function epubAssetUrl(base: string, inner: string): string {
  const encoded = inner
    .split("/")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return `${base}/${encoded}`;
}

/** Resolves a possibly relative reference against the directory of `from`. */
export function resolveInner(from: string, reference: string): string | null {
  const trimmed = reference.trim();
  if (
    !trimmed ||
    trimmed.startsWith("#") ||
    trimmed.includes("://") ||
    trimmed.startsWith("data:")
  ) {
    return null;
  }
  const [path] = trimmed.split("#");
  const baseDir = from.includes("/") ? from.slice(0, from.lastIndexOf("/")) : "";
  const combined = path.startsWith("/") ? path.slice(1) : baseDir ? `${baseDir}/${path}` : path;
  const segments: string[] = [];
  for (const segment of combined.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") segments.pop();
    else segments.push(segment);
  }
  return segments.join("/");
}
