import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/index.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("#root element missing from index.html");
}

// Plain-browser dev: stand in for the Rust commands (src/dev/mockBackend.ts).
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  await import("./dev/mockBackend");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
