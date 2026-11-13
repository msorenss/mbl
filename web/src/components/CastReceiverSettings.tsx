import { createSignal } from "solid-js";
import { castManager } from "../utils/cast";
import { receiverConfiguration, saveReceiverConfiguration } from "../utils/castReceiverConfig";

export default function CastReceiverSettings() {
  const config = receiverConfiguration();
  const [appId, setAppId] = createSignal(config.custom ? config.appId : "");
  const [notice, setNotice] = createSignal("");
  const apply = (value: string | null) => {
    if (castManager.getState().isCasting) {
      setNotice("Disconnect Cast before changing receiver applications.");
      return;
    }
    try {
      saveReceiverConfiguration(value);
      window.location.reload();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Could not save receiver settings.");
    }
  };
  return (
    <details>
      <summary>Cast receiver application (advanced)</summary>
      <p>
        Current:{" "}
        {config.custom
          ? `MBL custom receiver (${config.appId})`
          : "Google standard receiver — safe compatibility profile"}
        .
      </p>
      <p>
        This setting applies to this browser. Use a registered MBL receiver application ID. Changing
        applications reloads this page and requires reconnecting Cast. No device model selection is
        needed.
      </p>
      <label for="cast-receiver-app-id">Registered application ID</label>
      <input
        id="cast-receiver-app-id"
        value={appId()}
        maxlength={8}
        onInput={(event) => setAppId(event.currentTarget.value.trim())}
      />
      <button type="button" onClick={() => apply(appId())}>
        Use MBL receiver and reload
      </button>
      <button type="button" onClick={() => apply("CC1AD845")}>
        Use standard compatibility receiver and reload
      </button>
      <button type="button" onClick={() => apply(null)}>
        Use deployment default and reload
      </button>
      <p role="status">{notice()}</p>
    </details>
  );
}
