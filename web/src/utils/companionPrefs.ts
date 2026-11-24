import type { components } from "../api/schema";

export type CompanionMode = components["schemas"]["CompanionMode"];
export type CompanionFrequency = components["schemas"]["CompanionFrequency"];

/** The viewer's companion choices, kept in this browser and sent when playback starts. */
export const MODE_KEY = "mbl.companion.mode";
export const FREQUENCY_KEY = "mbl.companion.frequency";

export const MODES: { value: CompanionMode; label: string }[] = [
  { value: "on", label: "Comments on" },
  { value: "replies", label: "Replies only" },
  { value: "off", label: "Comments off" },
];

export const FREQUENCIES: { value: CompanionFrequency; label: string }[] = [
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];

export function storedMode(): CompanionMode {
  try {
    const value = localStorage.getItem(MODE_KEY);
    if (value === "on" || value === "replies" || value === "off") return value;
  } catch {}
  return "on";
}

export function storedFrequency(): CompanionFrequency {
  try {
    const value = localStorage.getItem(FREQUENCY_KEY);
    if (value === "low" || value === "medium" || value === "high") return value;
  } catch {}
  return "medium";
}

export function storePref(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {}
}
