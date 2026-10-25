import { describe, expect, it } from "vitest";
import { translations } from "./index";

describe("i18n Translations", () => {
  it("has matching keys across en, sv, and de", () => {
    const enKeys = Object.keys(translations.en).sort();
    const svKeys = Object.keys(translations.sv).sort();
    const deKeys = Object.keys(translations.de).sort();

    expect(svKeys).toEqual(enKeys);
    expect(deKeys).toEqual(enKeys);
  });

  it("provides non-empty strings for all keys", () => {
    for (const [locale, dict] of Object.entries(translations)) {
      for (const [key, val] of Object.entries(dict)) {
        expect(val.length, `${locale}.${key} is empty`).toBeGreaterThan(0);
      }
    }
  });
});
