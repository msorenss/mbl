import { describe, expect, it } from "vitest";
import { localizeDomSubtree, SUPPORTED_LOCALES, translations } from "./index";

describe("i18n Translations", () => {
  it("supports all 23 languages (including Tagalog, Bisaya, and Waray-Waray) with identical key sets", () => {
    expect(SUPPORTED_LOCALES.length).toBe(23);
    const enKeys = Object.keys(translations.en).sort();
    expect(enKeys.length).toBeGreaterThan(200);

    for (const loc of SUPPORTED_LOCALES) {
      const locDict = translations[loc.code];
      expect(locDict, `Missing dictionary for ${loc.code}`).toBeDefined();
      const locKeys = Object.keys(locDict).sort();
      expect(locKeys, `Key mismatch in locale ${loc.code}`).toEqual(enKeys);
    }
  });

  it("provides non-empty strings for all keys across all 23 locales", () => {
    for (const [locale, dict] of Object.entries(translations)) {
      for (const [key, val] of Object.entries(dict)) {
        expect(val.length, `${locale}.${key} is empty`).toBeGreaterThan(0);
      }
    }
  });

  it("localizes DOM text nodes and attributes across languages and restores English cleanly", () => {
    const container = document.createElement("div");
    container.innerHTML = `
      <h1>Who's watching?</h1>
      <button title="Close">Save</button>
      <input placeholder="Username" />
    `;

    localizeDomSubtree(container, "sv");
    expect(container.querySelector("h1")?.textContent).toBe("Vem tittar?");
    expect(container.querySelector("button")?.textContent).toBe("Spara");
    expect(container.querySelector("button")?.getAttribute("title")).toBe("Stäng");
    expect(container.querySelector("input")?.getAttribute("placeholder")).toBe("Användarnamn");

    localizeDomSubtree(container, "tl");
    expect(container.querySelector("h1")?.textContent).toBe("Sino ang nanonood?");
    expect(container.querySelector("button")?.textContent).toBe("I-save");

    localizeDomSubtree(container, "ceb");
    expect(container.querySelector("h1")?.textContent).toBe("Kinsa ang nagtan-aw?");

    localizeDomSubtree(container, "war");
    expect(container.querySelector("h1")?.textContent).toBe("Hin-o an nagkikita?");

    localizeDomSubtree(container, "en");
    expect(container.querySelector("h1")?.textContent).toBe("Who's watching?");
    expect(container.querySelector("button")?.textContent).toBe("Save");
    expect(container.querySelector("button")?.getAttribute("title")).toBe("Close");
    expect(container.querySelector("input")?.getAttribute("placeholder")).toBe("Username");
  });
});
