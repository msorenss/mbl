import { createSignal } from "solid-js";

export interface BeforeInstallPromptEvent extends Event {
  readonly platforms?: string[];
  readonly userChoice: Promise<{ outcome: "accepted" | "dismissed"; platform: string }>;
  prompt(): Promise<void>;
}

const [deferredPrompt, setDeferredPrompt] = createSignal<BeforeInstallPromptEvent | null>(null);
const [isStandalone, setIsStandalone] = createSignal<boolean>(false);

function detectStandalone(): boolean {
  if (typeof window === "undefined") return false;
  const mediaStandalone = window.matchMedia?.("(display-mode: standalone)")?.matches ?? false;
  const iosStandalone =
    Boolean((window.navigator as Navigator & { standalone?: boolean }).standalone) === true;
  return mediaStandalone || iosStandalone;
}

export function isIosSafariInstallCandidate(): boolean {
  if (typeof window === "undefined" || isStandalone()) return false;
  const ua = window.navigator.userAgent || "";
  const isIosOrIpadOs =
    /iPhone|iPad|iPod/i.test(ua) ||
    (window.navigator.platform === "MacIntel" && window.navigator.maxTouchPoints > 1);
  return isIosOrIpadOs && !deferredPrompt();
}

let initialized = false;

export function initPwaSupport(): void {
  if (initialized || typeof window === "undefined") return;
  initialized = true;

  setIsStandalone(detectStandalone());

  const mql = window.matchMedia?.("(display-mode: standalone)");
  mql?.addEventListener?.("change", (e) => {
    setIsStandalone(e.matches);
  });

  window.addEventListener("beforeinstallprompt", (event) => {
    event.preventDefault();
    setDeferredPrompt(event as BeforeInstallPromptEvent);
  });

  window.addEventListener("appinstalled", () => {
    setDeferredPrompt(null);
    setIsStandalone(true);
  });

  if ("serviceWorker" in window.navigator && import.meta.env.PROD) {
    window.addEventListener("load", () => {
      window.navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => {
        // Ignore SW registration failure on unsupported / non-secure contexts
      });
    });
  }
}

export function canInstallPwa(): boolean {
  return !isStandalone() && deferredPrompt() !== null;
}

export function isStandaloneApp(): boolean {
  return isStandalone();
}

export async function promptInstallPwa(): Promise<boolean> {
  const promptEvent = deferredPrompt();
  if (!promptEvent) return false;
  try {
    await promptEvent.prompt();
    const choice = await promptEvent.userChoice;
    if (choice.outcome === "accepted") {
      setDeferredPrompt(null);
      return true;
    }
  } catch {
    // User dismissed or browser rejected
  }
  return false;
}

/** Test helper to reset internal PWA state between unit tests */
export function __resetPwaForTests(): void {
  initialized = false;
  setDeferredPrompt(null);
  setIsStandalone(false);
}
