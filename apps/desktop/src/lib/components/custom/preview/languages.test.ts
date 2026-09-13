import { describe, expect, test } from "bun:test";
import { GRAMMAR_LOADERS } from "./grammars.js";
import {
  isHtmlName,
  isMarkdownName,
  kindLabel,
  LANGUAGE_IDS,
  languageFor,
  languageForFence,
} from "./languages.js";

describe("languageFor", () => {
  test("maps extensions case-insensitively", () => {
    expect(languageFor("main.RS")).toBe("rust");
    expect(languageFor("App.svelte")).toBe("svelte");
    expect(languageFor("View.tsx")).toBe("tsx");
    expect(languageFor("index.ts")).toBe("typescript");
    expect(languageFor("README.md")).toBe("markdown");
  });

  test("maps JSON and TOML to their own grammars", () => {
    expect(languageFor("package.json")).toBe("json");
    expect(languageFor("Cargo.toml")).toBe("toml");
  });

  test("maps known file names", () => {
    expect(languageFor("Dockerfile")).toBe("dockerfile");
    expect(languageFor("Makefile")).toBe("make");
    expect(languageFor(".gitignore")).toBe("shellscript");
    expect(languageFor(".env")).toBe("dotenv");
  });

  test("falls back to plaintext", () => {
    expect(languageFor("notes.txt")).toBe("plaintext");
    expect(languageFor("LICENSE")).toBe("plaintext");
    expect(languageFor("data.unknownext")).toBe("plaintext");
  });
});

describe("languageForFence", () => {
  test("maps fence aliases", () => {
    expect(languageForFence("bash")).toBe("shellscript");
    expect(languageForFence("ts")).toBe("typescript");
    expect(languageForFence("rust")).toBe("rust");
    expect(languageForFence("JSON")).toBe("json");
    expect(languageForFence("svelte")).toBe("svelte");
  });

  test("falls back to plaintext", () => {
    expect(languageForFence("")).toBe("plaintext");
    expect(languageForFence("unknownlang")).toBe("plaintext");
  });
});

describe("isMarkdownName", () => {
  test("detects markdown extensions", () => {
    expect(isMarkdownName("README.md")).toBe(true);
    expect(isMarkdownName("guide.MARKDOWN")).toBe(true);
    expect(isMarkdownName("page.mdx")).toBe(true);
    expect(isMarkdownName("main.rs")).toBe(false);
    expect(isMarkdownName("md")).toBe(false);
  });
});

describe("isHtmlName", () => {
  test("detects html extensions", () => {
    expect(isHtmlName("index.html")).toBe(true);
    expect(isHtmlName("page.HTM")).toBe(true);
    expect(isHtmlName("main.rs")).toBe(false);
    expect(isHtmlName("html")).toBe(false);
  });
});

describe("kindLabel", () => {
  test("labels folders and known types", () => {
    expect(kindLabel("docs", "directory")).toBe("Folder");
    expect(kindLabel("README.md", "text")).toBe("Markdown Document");
    expect(kindLabel("Cargo.toml", "text")).toBe("TOML Document");
  });

  test("falls back to extension, then Document", () => {
    expect(kindLabel("photo.PNG", "binary")).toBe("PNG File");
    expect(kindLabel("archive.tar.gz", "binary")).toBe("GZ File");
    expect(kindLabel("LICENSE", "text")).toBe("Document");
    expect(kindLabel(".gitignore", "text")).toBe("Document");
  });
});

describe("grammar coverage", () => {
  test("every mapped language has a grammar loader", () => {
    const missing = LANGUAGE_IDS.filter((id) => !(id in GRAMMAR_LOADERS));
    expect(missing).toEqual([]);
  });
});
