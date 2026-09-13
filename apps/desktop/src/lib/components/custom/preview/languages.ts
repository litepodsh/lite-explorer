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

export function kindLabel(name: string, kind: PreviewKind): string {
  if (kind === "directory") return "Folder";
  const extension = extensionOf(name);
  return (
    KIND_LABEL_BY_EXTENSION[extension] ??
    (extension ? `${extension.toUpperCase()} File` : "Document")
  );
}
