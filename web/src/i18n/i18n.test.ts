import { describe, expect, it } from "vitest";
import { translations } from "./index";

describe("i18n Translations", () => {
  it("has matching keys across en, sv, de, fr, and es", () => {
    const enKeys = Object.keys(translations.en).sort();
    const svKeys = Object.keys(translations.sv).sort();
    const deKeys = Object.keys(translations.de).sort();
    const frKeys = Object.keys(translations.fr).sort();
    const esKeys = Object.keys(translations.es).sort();

    expect(svKeys).toEqual(enKeys);
    expect(deKeys).toEqual(enKeys);
    expect(frKeys).toEqual(enKeys);
    expect(esKeys).toEqual(enKeys);
  });

  it("provides non-empty strings for all keys", () => {
    for (const [locale, dict] of Object.entries(translations)) {
      for (const [key, val] of Object.entries(dict)) {
        expect(val.length, `${locale}.${key} is empty`).toBeGreaterThan(0);
      }
    }
  });
});
