import React from "react";
import ReactDOM from "react-dom/client";
import { RegionCaptureOverlay } from "./RegionCaptureOverlay";
import "./region-capture.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RegionCaptureOverlay />
  </React.StrictMode>,
);
