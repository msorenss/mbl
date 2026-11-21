import { createResource, createSignal, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

type ArrLink = components["schemas"]["ArrLink"];
type Service = ArrLink["service"];

const NAME: Record<Service, string> = { radarr: "Radarr", sonarr: "Sonarr", lidarr: "Lidarr" };

let cached: { at: number; links: Promise<ArrLink[]> } | undefined;

/** Download services with an address and a key (admin only), cached for a minute. */
export function configuredArrServices(): Promise<ArrLink[]> {
  if (!cached || Date.now() - cached.at > 60_000) {
    const links = client
      .GET("/api/v1/requests/arr-links")
      .then((r) => r.data ?? [])
      .catch(() => []);
    cached = { at: Date.now(), links };
  }
  return cached.links;
}

/** Forget the cached list, e.g. after the admin changes a service's settings. */
export function forgetArrServices() {
  cached = undefined;
}

/**
 * "Open in Radarr/Sonarr/Lidarr" for admins. Shown only when that service has an address and a
 * key. Opens the title itself when the service has it, otherwise the service's start page.
 */
export default function ArrLinkButton(props: {
  service: Service;
  size?: "sm" | "md";
  /** A media request, or a library movie/show. */
  target: { request: number } | { item: number };
}) {
  const [services] = createResource(configuredArrServices);
  const [opening, setOpening] = createSignal(false);
  const start = () => services()?.find((s) => s.service === props.service);

  const open = async () => {
    const fallback = start()?.url;
    // Open the window right away so the browser does not treat it as an unwanted pop-up.
    const win = window.open("", "_blank");
    if (win) win.opener = null;
    setOpening(true);
    let url = fallback;
    try {
      const t = props.target;
      const result =
        "request" in t
          ? await client.GET("/api/v1/requests/{id}/arr-link", {
              params: { path: { id: t.request } },
            })
          : await client.GET("/api/v1/items/{id}/arr-link", { params: { path: { id: t.item } } });
      if (result.data?.url) url = result.data.url;
    } catch {
      // Keep the start page.
    } finally {
      setOpening(false);
    }
    if (!url) {
      win?.close();
      return;
    }
    if (win) win.location.href = url;
    else window.open(url, "_blank", "noopener");
  };

  return (
    <Show when={start()}>
      <Button
        size={props.size ?? "sm"}
        variant="secondary"
        disabled={opening()}
        onClick={() => void open()}
      >
        {`Open in ${NAME[props.service]}`}
      </Button>
    </Show>
  );
}
