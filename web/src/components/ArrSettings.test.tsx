import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import ArrSettings from "./ArrSettings";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const empty = { enabled: false, url: "", hasApiKey: false, rootFolder: "" };

it("tests the connection, fills folder and profile, and saves without echoing the key", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: empty, response: new Response() } as never);
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: {
      version: "5.1.0",
      rootFolders: ["/media/movies"],
      metadataProfiles: [],
      qualityProfiles: [{ id: 4, name: "HD-1080p" }],
    },
    response: new Response(),
  } as never);
  const put = vi.spyOn(client, "PUT").mockResolvedValue({
    data: { ...empty, enabled: true, hasApiKey: true, url: "http://nas:7878" },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(ArrSettings, {}), document.body);
  await tick();
  const radarr = document.querySelectorAll("section")[0];
  const inputs = radarr.querySelectorAll<HTMLInputElement>("input");
  inputs[0].value = "http://nas:7878";
  inputs[0].dispatchEvent(new Event("input", { bubbles: true }));
  inputs[1].value = "secret";
  inputs[1].dispatchEvent(new Event("input", { bubbles: true }));
  const button = (label: string) =>
    [...radarr.querySelectorAll("button")].find((b) => b.textContent?.trim() === label);
  button("Test connection")?.click();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/requests/arr/{service}/test", {
    params: { path: { service: "radarr" } },
    body: { url: "http://nas:7878", apiKey: "secret" },
  });
  expect(radarr.textContent).toContain("Connected to Radarr 5.1.0.");
  expect(radarr.querySelector("select")?.value).toBe("/media/movies");
  const toggle = radarr.querySelector<HTMLInputElement>('input[type="checkbox"]');
  toggle?.click();
  button("Save")?.click();
  await tick();
  expect(put).toHaveBeenCalledWith("/api/v1/requests/arr/{service}", {
    params: { path: { service: "radarr" } },
    body: {
      enabled: true,
      url: "http://nas:7878",
      apiKey: "secret",
      rootFolder: "/media/movies",
      qualityProfileId: 4,
    },
  });
  expect(radarr.textContent).toContain("Approved movies are sent to Radarr.");
  expect(inputs[1].value).toBe("");
});

it("shows why the connection failed", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({ data: empty, response: new Response() } as never);
  vi.spyOn(client, "POST").mockResolvedValue({
    error: { error: { message: "external service: The API key was rejected" } },
    response: new Response(null, { status: 503 }),
  } as never);
  dispose = render(() => createComponent(ArrSettings, {}), document.body);
  await tick();
  const sonarr = document.querySelectorAll("section")[1];
  const url = sonarr.querySelector<HTMLInputElement>("input");
  if (url) {
    url.value = "http://nas:8989";
    url.dispatchEvent(new Event("input", { bubbles: true }));
  }
  [...sonarr.querySelectorAll("button")].find((b) => b.textContent === "Test connection")?.click();
  await tick();
  expect(sonarr.querySelector('[role="alert"]')?.textContent).toBe("The API key was rejected");
});
