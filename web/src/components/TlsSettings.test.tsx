import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import client from "../api/client";
import TlsSettings from "./TlsSettings";

vi.mock("../context/ToastContext", () => ({
  useToast: () => ({ showToast: vi.fn() }),
}));

let dispose: (() => void) | undefined;

function button(text: string) {
  const match = [...document.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!match) throw new Error(`Missing button: ${text}`);
  return match;
}

describe("TlsSettings", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    vi.spyOn(client, "GET").mockResolvedValue({
      data: {
        enabled: false,
        port: 8443,
        mode: "none",
        hostnames: [],
        caAvailable: false,
        hasCertificate: false,
        expiresAt: null,
        acmeEmail: null,
      },
      response: new Response(),
    } as never);
  });

  afterEach(() => {
    dispose?.();
    dispose = undefined;
    vi.restoreAllMocks();
  });

  it("renders Local CA, Tailscale, and Let's Encrypt tabs and issues Local CA certificate", async () => {
    const postSpy = vi.spyOn(client, "POST").mockResolvedValue({
      data: {
        enabled: false,
        port: 8443,
        mode: "local_ca",
        hostnames: ["192.168.68.147", "localhost"],
        caAvailable: true,
        hasCertificate: true,
        expiresAt: 1999999999,
        acmeEmail: null,
      },
      response: new Response(),
    } as never);

    dispose = render(() => createComponent(TlsSettings, {}), document.body);
    await Promise.resolve();

    expect(document.body.textContent).toContain("HTTPS & TLS Certificates");
    expect(document.body.textContent).toContain("1. Local CA (LAN / IP)");
    expect(document.body.textContent).toContain("2. Tailscale (*.ts.net)");
    expect(document.body.textContent).toContain("3. Let's Encrypt (ACME)");

    const input = document.getElementById("tls-local-hosts") as HTMLInputElement;
    input.value = "192.168.68.147, localhost";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.form?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await Promise.resolve();

    expect(postSpy).toHaveBeenCalledWith("/api/v1/system/tls/local-ca", {
      body: { hostnames: ["192.168.68.147", "localhost"] },
    });
  });

  it("switches to Tailscale and Let's Encrypt tabs", async () => {
    dispose = render(() => createComponent(TlsSettings, {}), document.body);
    await Promise.resolve();

    button("2. Tailscale (*.ts.net)").click();
    expect(document.body.textContent).toContain("Fetch Tailscale Certificate");

    button("3. Let's Encrypt (ACME)").click();
    expect(document.body.textContent).toContain("Activate Let's Encrypt Certificate");
  });
});
