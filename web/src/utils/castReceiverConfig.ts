const KEY = "mbl.cast.receiver.v1";
/** Google's Default Media Receiver: no MBL code, safe compatibility profile. */
export const STANDARD_RECEIVER_APP_ID = "CC1AD845";
/** The published MBL receiver, hosted from the project's GitHub Pages site. */
export const MBL_RECEIVER_APP_ID = "422EE6A6";
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
  const appId = validReceiverAppId(configured) ? configured.toUpperCase() : MBL_RECEIVER_APP_ID;
  return { appId, custom: appId !== STANDARD_RECEIVER_APP_ID };
}
export function saveReceiverConfiguration(value: string | null) {
  if (value !== null && !validReceiverAppId(value))
    throw new Error("Enter an eight-character hexadecimal Cast application ID.");
  if (value === null) localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, value.toUpperCase());
}
