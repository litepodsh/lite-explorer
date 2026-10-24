import { describe, expect, test } from "bun:test";
import { applyAppearance, isLightTheme, resolveTheme } from "./appearance.js";
import { defaultSettings } from "./settings.js";

describe("resolveTheme", () => {
  test("keeps a manual preset unchanged", () => {
    expect(resolveTheme("oled", "manual", false)).toBe("oled");
    expect(resolveTheme("explorer-light", "manual", true)).toBe("explorer-light");
  });

  test("flips within the Explorer pair", () => {
    expect(resolveTheme("dark", "system", false)).toBe("explorer-light");
    expect(resolveTheme("explorer-light", "system", true)).toBe("dark");
  });

  test("flips within the Paper pair", () => {
    expect(resolveTheme("off-black", "system", false)).toBe("off-white");
    expect(resolveTheme("off-white", "system", true)).toBe("off-black");
  });

  test("flips within the Contrast pair", () => {
    expect(resolveTheme("oled", "system", false)).toBe("light");
    expect(resolveTheme("light", "system", true)).toBe("oled");
  });
});

test("isLightTheme classifies the light side of every pair", () => {
  expect(isLightTheme("explorer-light")).toBe(true);
  expect(isLightTheme("off-white")).toBe(true);
  expect(isLightTheme("light")).toBe(true);
  expect(isLightTheme("dark")).toBe(false);
  expect(isLightTheme("off-black")).toBe(false);
  expect(isLightTheme("oled")).toBe(false);
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

test("applyAppearance follows the system inside the chosen pair", () => {
  const root = { dataset: {}, style: {} } as unknown as HTMLElement;
  const document = { documentElement: root } as Document;
  const settings = {
    ...defaultSettings({ dev: false }),
    theme: "dark" as const,
    themeMode: "system" as const,
  };

  applyAppearance(settings, document, true);
  expect(root.dataset.theme).toBe("dark");

  applyAppearance(settings, document, false);
  expect(root.dataset.theme).toBe("explorer-light");
  expect(root.style.colorScheme).toBe("light");
});
