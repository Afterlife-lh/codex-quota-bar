import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { initializeWindowActivity } from "./windowActivity";
import "./styles.css";

const windowLabel = getCurrentWindow().label;

void initializeWindowActivity(windowLabel).catch((error: unknown) => {
  console.error("Window activity initialization failed", error);
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App windowLabel={windowLabel} />
  </React.StrictMode>,
);
