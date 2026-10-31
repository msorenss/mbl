import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import { castDiagnostics } from "../utils/castDiagnostics";
import CastDiagnostics from "./CastDiagnostics";

vi.mock("../i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
});
it("updates the visible report and copies only its sanitized contents", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  castDiagnostics.start();
  dispose = render(() => createComponent(CastDiagnostics, {}), document.body);
  castDiagnostics.record("load.send", { url: "http://tv.test/master.m3u8?token=secret" });
  const report = document.querySelector("textarea") as HTMLTextAreaElement;
  expect(report.value).toContain("load.send");
  expect(report.value).not.toContain("token=secret");
  document.querySelector<HTMLButtonElement>("button")?.click();
  await Promise.resolve();
  expect(writeText).toHaveBeenCalledWith(report.value);
  expect(document.body.textContent).toContain("castDiagnosticsCopied");
});
it("offers selectable text if clipboard access is unavailable", async () => {
  vi.stubGlobal("navigator", {});
  dispose = render(() => createComponent(CastDiagnostics, {}), document.body);
  document.querySelector<HTMLButtonElement>("button")?.click();
  await Promise.resolve();
  expect(document.body.textContent).toContain("castDiagnosticsFallback");
  expect(document.querySelector<HTMLTextAreaElement>("textarea")?.readOnly).toBe(true);
});
