// MBL App Shell Service Worker — enables standalone mobile & tablet PWA installation
// Strictly bypasses API, media streams, images, Range requests, and MCP endpoints.
const CACHE_NAME = "mbl-shell-v2";
const PRECACHE_URLS = [
  "/",
  "/manifest.webmanifest",
  "/favicon.svg",
  "/icon-192.png",
  "/apple-touch-icon.png",
];

self.addEventListener("install", (event) => {
  self.skipWaiting();
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(PRECACHE_URLS).catch(() => {})),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  if (request.headers.has("range")) return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  // Never intercept API, streaming, artwork, Subsonic, or MCP calls
  if (
    url.pathname.startsWith("/cast-receiver/") ||
    url.pathname.startsWith("/api/") ||
    url.pathname.startsWith("/stream/") ||
    url.pathname.startsWith("/images/") ||
    url.pathname.startsWith("/rest/") ||
    url.pathname.startsWith("/mcp") ||
    url.pathname === "/health"
  ) {
    return;
  }

  // SPA navigation: network-first with cached app shell fallback
  if (request.mode === "navigate") {
    event.respondWith(
      fetch(request)
        .then((response) => {
          if (response.ok) {
            const copy = response.clone();
            caches.open(CACHE_NAME).then((cache) => cache.put("/", copy));
          }
          return response;
        })
        .catch(() => caches.match("/") || caches.match("/index.html")),
    );
    return;
  }

  // Immutable Vite hashed assets + icons: cache-first with network fallback
  if (
    url.pathname.startsWith("/assets/") ||
    url.pathname.endsWith(".png") ||
    url.pathname.endsWith(".svg") ||
    url.pathname.endsWith(".webmanifest")
  ) {
    event.respondWith(
      caches.match(request).then(
        (cached) =>
          cached ||
          fetch(request).then((response) => {
            if (response.ok) {
              const copy = response.clone();
              caches.open(CACHE_NAME).then((cache) => cache.put(request, copy));
            }
            return response;
          }),
      ),
    );
  }
});
