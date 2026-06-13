import React from "react";
import ReactDOM from "react-dom/client";
import BrainOverlayApp from "./BrainOverlayApp";
import "@/i18n";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BrainOverlayApp />
  </React.StrictMode>,
);
