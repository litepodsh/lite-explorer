import { describe, expect, test } from "bun:test";
import { applyAppearance, resolveTheme } from "./appearance.js";
import { defaultSettings } from "./settings.js";

describe("resolveTheme", () => {
  test("keeps a manual preset and resolves system to light or dark", () => {
    expect(resolveTheme("oled", "manual", false)).toBe("oled");
    expect(resolveTheme("oled", "system", false)).toBe("light");
    expect(resolveTheme("oled", "system", true)).toBe("dark");
  });
});

test("applyAppearance writes root datasets and native color scheme", () => {
  const root = { dataset: {}, style: {} } as unknown as HTMLElement;
  const document = { documentElement: root } as Document;
  const settings = {
    ...defaultSettings({ dev: false }),
    theme: "oled" as const,
    radius: "round" as const,
    elevation: "lifted" as const,
    texture: "paper" as const,
  };

  applyAppearance(settings, document);

  expect(root.dataset.theme).toBe("oled");
  expect(root.dataset.radius).toBe("round");
  expect(root.dataset.elevation).toBe("lifted");
  expect(root.dataset.texture).toBe("paper");
  expect(root.style.colorScheme).toBe("dark");
});
