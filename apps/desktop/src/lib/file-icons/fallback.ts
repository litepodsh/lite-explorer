/**
 * Extension to icon-category mapping used when a native OS icon is not
 * available (remote, network, archive-inner entries, or extraction failures).
 *
 * Kept free of Svelte imports so it stays unit-testable; `entry-icon.svelte`
 * maps each category to a Lucide icon and colour.
 */
export type IconCategory =
  | "archive"
  | "image"
  | "video"
  | "audio"
  | "code"
  | "text"
  | "pdf"
  | "document"
  | "sheet"
  | "slides"
  | "font"
  | "default";

export type CustomIcon =
  | "folder"
  | "file"
  | "file_archive"
  | "file_config"
  | "file_document"
  | "file_metadata"
  | "file_pdf"
  | "file_spreadsheet"
  | "file_subtitle";

const BY_EXTENSION: Record<string, IconCategory> = {};

function register(category: IconCategory, extensions: string[]): void {
  for (const extension of extensions) BY_EXTENSION[extension] = category;
}

register("archive", [
  "zip",
  "tar",
  "gz",
  "tgz",
  "bz2",
  "tbz2",
  "xz",
  "txz",
  "7z",
  "rar",
  "zst",
  "lz",
  "lzma",
  "dmg",
  "iso",
  "cab",
  "deb",
  "rpm",
  "apk",
  "jar",
  "war",
]);
register("image", [
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "bmp",
  "tif",
  "tiff",
  "heic",
  "heif",
  "avif",
  "ico",
  "svg",
  "psd",
  "raw",
  "cr2",
  "nef",
]);
register("video", ["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "m2ts"]);
register("audio", [
  "mp3",
  "wav",
  "flac",
  "aac",
  "ogg",
  "oga",
  "opus",
  "m4a",
  "aiff",
  "aif",
  "wma",
  "mid",
  "midi",
]);
register("code", [
  "js",
  "mjs",
  "cjs",
  "ts",
  "mts",
  "jsx",
  "tsx",
  "svelte",
  "vue",
  "json",
  "jsonc",
  "html",
  "htm",
  "css",
  "scss",
  "sass",
  "less",
  "rs",
  "py",
  "rb",
  "go",
  "java",
  "kt",
  "kts",
  "c",
  "h",
  "cc",
  "cpp",
  "cxx",
  "hpp",
  "cs",
  "sh",
  "bash",
  "zsh",
  "fish",
  "ps1",
  "bat",
  "cmd",
  "yml",
  "yaml",
  "toml",
  "ini",
  "cfg",
  "conf",
  "xml",
  "plist",
  "sql",
  "php",
  "lua",
  "dart",
  "swift",
  "scala",
  "r",
  "ex",
  "exs",
  "elm",
  "clj",
  "hs",
  "tf",
  "hcl",
]);
register("text", [
  "txt",
  "text",
  "md",
  "markdown",
  "mdx",
  "rst",
  "log",
  "tsv",
  "srt",
  "vtt",
  "nfo",
]);
register("pdf", ["pdf"]);
register("document", ["doc", "docx", "odt", "rtf", "pages", "wpd", "tex"]);
register("sheet", ["xls", "xlsx", "xlsm", "ods", "numbers", "csv"]);
register("slides", ["ppt", "pptx", "pptm", "odp", "key"]);
register("font", ["ttf", "otf", "woff", "woff2", "eot"]);

const BY_NAME: Record<string, IconCategory> = {
  makefile: "code",
  dockerfile: "code",
  license: "text",
  readme: "text",
  changelog: "text",
};

/** Lowercased extension without the dot; empty when the name has none. */
export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return "";
  return name.slice(dot + 1).toLowerCase();
}

export function categoryFor(name: string): IconCategory {
  const extension = extensionOf(name);
  if (extension) return BY_EXTENSION[extension] ?? "default";
  return BY_NAME[name.toLowerCase()] ?? "default";
}

/** The supplied SVG set only replaces the matching file types; other known
 * types keep their native or Lucide icon. */
export function customIconFor(name: string, directory = false): CustomIcon | undefined {
  if (directory) return "folder";
  switch (extensionOf(name)) {
    case "zip":
    case "tar":
    case "gz":
    case "tgz":
    case "bz2":
    case "tbz2":
    case "xz":
    case "txz":
    case "7z":
    case "rar":
      return "file_archive";
    case "conf":
    case "cfg":
    case "ini":
    case "json":
    case "jsonc":
    case "yml":
    case "yaml":
    case "toml":
    case "xml":
    case "plist":
      return "file_config";
    case "doc":
    case "docx":
    case "odt":
    case "rtf":
    case "pages":
    case "wpd":
    case "tex":
      return "file_document";
    case "pdf":
      return "file_pdf";
    case "srt":
    case "vtt":
      return "file_subtitle";
    case "xls":
    case "xlsx":
    case "xlsm":
    case "ods":
    case "numbers":
    case "csv":
      return "file_spreadsheet";
    case "xmp":
      return "file_metadata";
    default:
      return categoryFor(name) === "default" ? "file" : undefined;
  }
}
