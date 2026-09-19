import type { PreviewKind } from "./types.js";

// Shiki grammar ids (see grammars.ts).
const LANGUAGE_BY_EXTENSION: Record<string, string> = {
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  tsx: "tsx",
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  jsx: "jsx",
  json: "json",
  jsonc: "jsonc",
  json5: "json5",
  svelte: "svelte",
  vue: "vue",
  html: "html",
  htm: "html",
  css: "css",
  scss: "scss",
  less: "less",
  md: "markdown",
  markdown: "markdown",
  mdx: "mdx",
  rs: "rust",
  py: "python",
  go: "go",
  java: "java",
  kt: "kotlin",
  kts: "kotlin",
  swift: "swift",
  c: "c",
  h: "c",
  cc: "cpp",
  cpp: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  cs: "csharp",
  rb: "ruby",
  php: "php",
  sh: "shellscript",
  bash: "shellscript",
  zsh: "shellscript",
  ps1: "powershell",
  yml: "yaml",
  yaml: "yaml",
  toml: "toml",
  ini: "ini",
  cfg: "ini",
  conf: "ini",
  xml: "xml",
  svg: "xml",
  plist: "xml",
  sql: "sql",
  lua: "lua",
  dart: "dart",
  ex: "elixir",
  exs: "elixir",
  graphql: "graphql",
  gql: "graphql",
  proto: "proto",
  r: "r",
  scala: "scala",
  dockerfile: "dockerfile",
  nu: "nushell",
  diff: "diff",
  patch: "diff",
  zig: "zig",
  hs: "haskell",
  log: "log",
};

// Delimited data files are colored per column, not per token (see rainbow-csv.ts).
const CSV_EXTENSIONS = ["csv", "tsv", "psv"];

// No gitignore grammar ships with Shiki; shellscript colors its # comments.
const LANGUAGE_BY_FILE_NAME: Record<string, string> = {
  dockerfile: "dockerfile",
  makefile: "make",
  ".gitignore": "shellscript",
  ".bashrc": "shellscript",
  ".zshrc": "shellscript",
  ".env": "dotenv",
  "cargo.lock": "toml",
};

const LANGUAGE_BY_FENCE: Record<string, string> = {
  shell: "shellscript",
  console: "shellscript",
  golang: "go",
};

export const LANGUAGE_IDS = [
  ...new Set([
    ...Object.values(LANGUAGE_BY_EXTENSION),
    ...Object.values(LANGUAGE_BY_FILE_NAME),
    ...Object.values(LANGUAGE_BY_FENCE),
  ]),
];

const KIND_LABEL_BY_EXTENSION: Record<string, string> = {
  md: "Markdown Document",
  markdown: "Markdown Document",
  mdx: "MDX Document",
  txt: "Plain Text",
  log: "Log File",
  json: "JSON Document",
  toml: "TOML Document",
  yml: "YAML Document",
  yaml: "YAML Document",
  rs: "Rust Source",
  ts: "TypeScript Source",
  js: "JavaScript Source",
  svelte: "Svelte Component",
  html: "HTML Document",
  css: "CSS Stylesheet",
  py: "Python Script",
  sh: "Shell Script",
  csv: "Comma-Separated Values",
  tsv: "Tab-Separated Values",
  psv: "Pipe-Separated Values",
  mp4: "MPEG-4 Video",
  m4v: "MPEG-4 Video",
  mov: "QuickTime Movie",
  webm: "WebM Video",
  mkv: "Matroska Video",
  avi: "AVI Video",
  mp3: "MP3 Audio",
  m4a: "AAC Audio",
  wav: "WAV Audio",
  flac: "FLAC Audio",
  ogg: "Ogg Audio",
  opus: "Opus Audio",
  otf: "OpenType Font",
  ttf: "TrueType Font",
  woff: "WOFF Font",
  woff2: "WOFF2 Font",
  epub: "EPUB Book",
  xlsx: "Excel Workbook",
  xlsm: "Excel Workbook",
  xltx: "Excel Template",
  xltm: "Excel Template",
  xls: "Excel 97-2003 Workbook",
  xlsb: "Excel Binary Workbook",
  ods: "OpenDocument Spreadsheet",
  ots: "OpenDocument Spreadsheet",
  fods: "OpenDocument Spreadsheet (flat)",
  rtf: "Rich Text Document",
  docx: "Word Document",
  docm: "Word Document",
  dotx: "Word Template",
  dotm: "Word Template",
  odt: "OpenDocument Text",
  ott: "OpenDocument Text Template",
  fodt: "OpenDocument Text (flat)",
  pptx: "PowerPoint Presentation",
  pptm: "PowerPoint Presentation",
  potx: "PowerPoint Template",
  potm: "PowerPoint Template",
  ppsx: "PowerPoint Show",
  ppsm: "PowerPoint Show",
  odp: "OpenDocument Presentation",
  otp: "OpenDocument Presentation Template",
  fodp: "OpenDocument Presentation (flat)",
  eml: "Email Message",
  emlx: "Apple Mail Message",
  mbox: "Mailbox",
  mbx: "Mailbox",
  vcf: "vCard Contact",
  vcard: "vCard Contact",
  ics: "iCalendar",
  ical: "iCalendar",
  ifb: "iCalendar",
  torrent: "BitTorrent Metainfo",
  ndjson: "JSON Lines",
  jsonl: "JSON Lines",
  diff: "Unified Diff",
  patch: "Patch",
  cbz: "Comic Book Archive",
  ipynb: "Jupyter Notebook",
  sqlite: "SQLite Database",
  sqlite3: "SQLite Database",
  db: "SQLite Database",
  srt: "SubRip Subtitles",
  vtt: "WebVTT Subtitles",
  webvtt: "WebVTT Subtitles",
  pem: "PEM File",
  crt: "X.509 Certificate",
  cer: "X.509 Certificate",
  der: "DER Certificate",
  cert: "X.509 Certificate",
  stl: "STL 3D Model",
  obj: "Wavefront OBJ",
  gltf: "glTF Model",
  glb: "glTF Binary",
  dae: "Collada Model",
  geojson: "GeoJSON",
  kml: "Keyhole Markup Language",
  gpx: "GPS Exchange Format",
  fb2: "FictionBook",
  pcap: "Packet Capture",
  iso: "Disc Image",
  msg: "Outlook Message",
  sketch: "Sketch Document",
  psd: "Photoshop Document",
  psb: "Photoshop Document",
  dcm: "DICOM Image",
  dicom: "DICOM Image",
  mobi: "Kindle Book",
  azw3: "Kindle Book",
  azw: "Kindle Book",
  avro: "Apache Avro",
  parquet: "Apache Parquet",
  arrow: "Apache Arrow",
  feather: "Arrow Feather",
  ipc: "Arrow IPC",
};

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function languageFor(name: string): string {
  return (
    LANGUAGE_BY_FILE_NAME[name.toLowerCase()] ??
    LANGUAGE_BY_EXTENSION[extensionOf(name)] ??
    "plaintext"
  );
}

export function languageForFence(fence: string): string {
  const key = fence.trim().toLowerCase();
  if (!key) return "plaintext";
  if (LANGUAGE_IDS.includes(key)) return key;
  return LANGUAGE_BY_FENCE[key] ?? LANGUAGE_BY_EXTENSION[key] ?? "plaintext";
}

export function isMarkdownName(name: string): boolean {
  return ["md", "markdown", "mdx"].includes(extensionOf(name));
}

export function isHtmlName(name: string): boolean {
  return ["html", "htm"].includes(extensionOf(name));
}

export function isCsvName(name: string): boolean {
  return CSV_EXTENSIONS.includes(extensionOf(name));
}

const SPREADSHEET_EXTENSIONS = [
  "xlsx",
  "xlsm",
  "xltx",
  "xltm",
  "xls",
  "xla",
  "xlam",
  "xlsb",
  "ods",
  "ots",
  "fods",
];

export function isSpreadsheetName(name: string): boolean {
  return SPREADSHEET_EXTENSIONS.includes(extensionOf(name));
}

const VIDEO_EXTENSIONS = ["mp4", "m4v", "mov", "webm", "ogv", "mkv", "avi"];
const AUDIO_EXTENSIONS = ["mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus"];

export function isVideoName(name: string): boolean {
  return VIDEO_EXTENSIONS.includes(extensionOf(name));
}

export function isAudioName(name: string): boolean {
  return AUDIO_EXTENSIONS.includes(extensionOf(name));
}

/** Whether a file previews through the streaming `media://` protocol. */
export function isMediaKind(kind: PreviewKind): boolean {
  return (
    kind === "image" ||
    kind === "pdf" ||
    kind === "video" ||
    kind === "audio" ||
    kind === "font" ||
    kind === "model"
  );
}

export function kindLabel(name: string, kind: PreviewKind): string {
  if (kind === "directory") return "Folder";
  const extension = extensionOf(name);
  return (
    KIND_LABEL_BY_EXTENSION[extension] ??
    (extension ? `${extension.toUpperCase()} File` : "Document")
  );
}
