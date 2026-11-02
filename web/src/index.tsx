/* @refresh reload */
import { render } from "solid-js/web";
import "./index.css";
import App from "./App.tsx";

if (typeof window !== "undefined") {
  console.info(
    `%c[MBL] Media Lounge Box v${__MBL_VERSION__} (build: ${__MBL_GIT_COMMIT__}, date: ${__MBL_BUILD_DATE__})`,
    "color: #3b82f6; font-weight: 700; font-size: 12px; padding: 2px 4px;",
  );
}

const root = document.getElementById("root");

if (root) {
  render(() => <App />, root);
}
