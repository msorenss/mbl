import { expect, it } from "vitest";
import { localizeDomSubtree, translations } from "./index";

it("loads the less common languages only when one is chosen", async () => {
  // English and the first five languages are there from the start; Japanese is not yet.
  expect(translations.en.save).toBe("Save");
  expect(translations.sv.save).toBe("Spara");
  expect((translations.ja as Record<string, string>).save).toBeUndefined();

  const container = document.createElement("div");
  container.innerHTML = "<button>Save</button>";
  localizeDomSubtree(container, "ja");
  await expect.poll(() => container.querySelector("button")?.textContent).not.toBe("Save");
  expect(translations.ja.save).toBe(container.querySelector("button")?.textContent);
});
