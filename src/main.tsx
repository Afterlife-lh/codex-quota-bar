import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { initializeWindowActivity } from "./windowActivity";
import "./styles.css";

void initializeWindowActivity().catch((error: unknown) => {
  console.error("Window activity initialization failed", error);
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App windowLabel={getCurrentWindow().label} />
  </React.StrictMode>,
);
