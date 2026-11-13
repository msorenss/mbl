const KEY = "mbl.cast.receiver.v1";
export function validReceiverAppId(value: string) {
  return /^[A-F0-9]{8}$/i.test(value);
}
export function receiverConfiguration(): { appId: string; custom: boolean } {
  let override: string | null = null;
  try {
    override = localStorage.getItem(KEY);
  } catch {
    /* Storage unavailable. */
  }
  const configured = override ?? import.meta.env.VITE_MBL_CAST_APP_ID ?? "";
  const appId = validReceiverAppId(configured) ? configured.toUpperCase() : "CC1AD845";
  return { appId, custom: appId !== "CC1AD845" };
}
export function saveReceiverConfiguration(value: string | null) {
  if (value !== null && !validReceiverAppId(value))
    throw new Error("Enter an eight-character hexadecimal Cast application ID.");
  if (value === null) localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, value.toUpperCase());
}
