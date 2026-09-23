import { describe, expect, test } from "bun:test";
import { isAppBundle } from "./bundles.js";

const app = { path: "/Applications/Safari.app", name: "Safari.app", is_directory: true };

describe("isAppBundle", () => {
  test("local .app directories on macOS", () => {
    expect(isAppBundle(app, "macos")).toBe(true);
    expect(isAppBundle({ ...app, name: "Tool.APP" }, "macos")).toBe(true);
  });

  test("not on other platforms", () => {
    expect(isAppBundle(app, "windows")).toBe(false);
    expect(isAppBundle(app, "linux")).toBe(false);
  });

  test("files, plain folders and archive entries are not bundles", () => {
    expect(isAppBundle({ ...app, is_directory: false }, "macos")).toBe(false);
    expect(isAppBundle({ ...app, name: "Apps", path: "/Apps" }, "macos")).toBe(false);
    expect(isAppBundle({ ...app, inner_path: "Safari.app/" }, "macos")).toBe(false);
  });
});
