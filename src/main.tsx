import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { appReady } from "./lib/ipc";
import { showStartupError } from "./lib/splash";
import "./styles/index.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("#root element missing from index.html");
}

// Plain-browser dev: stand in for the Rust commands (src/dev/mockBackend.ts).
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  await import("./dev/mockBackend");
}

// The splash in index.html stays up until the library is open; App takes it
// down once mounted.
try {
  await appReady();
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
} catch (err) {
  showStartupError(String(err));
}
