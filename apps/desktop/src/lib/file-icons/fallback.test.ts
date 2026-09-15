import { describe, expect, it } from "vitest";
import { categoryFor, extensionOf } from "./fallback.js";

describe("extensionOf", () => {
  it("returns the lowercased extension", () => {
    expect(extensionOf("Photo.JPG")).toBe("jpg");
    expect(extensionOf("archive.tar.gz")).toBe("gz");
  });

  it("returns empty for names without an extension", () => {
    expect(extensionOf("README")).toBe("");
    expect(extensionOf(".bashrc")).toBe("");
    expect(extensionOf("trailing.")).toBe("");
  });
});

describe("categoryFor", () => {
  it("maps common extensions to their category", () => {
    expect(categoryFor("song.mp3")).toBe("audio");
    expect(categoryFor("clip.mp4")).toBe("video");
    expect(categoryFor("bundle.zip")).toBe("archive");
    expect(categoryFor("main.rs")).toBe("code");
    expect(categoryFor("report.pdf")).toBe("pdf");
    expect(categoryFor("photo.png")).toBe("image");
  });

  it("falls back to default for unknown extensions", () => {
    expect(categoryFor("data.unknownext")).toBe("default");
  });

  it("recognizes extensionless names it knows", () => {
    expect(categoryFor("Makefile")).toBe("code");
    expect(categoryFor("LICENSE")).toBe("text");
    expect(categoryFor("something")).toBe("default");
  });
});
