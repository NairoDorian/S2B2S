import { createSignal, onSettled } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";

import ModelSelector from "../model-selector";
import UpdateChecker from "../update-checker";
import BrainIndicator from "./BrainIndicator";
import SystemMeters from "./SystemMeters";

function Footer() {
  const [version, setVersion] = createSignal("");

  onSettled(() => {
    getVersion()
      .then(setVersion)
      .catch((error) => {
        // Show no version rather than a made-up one.
        console.error("Failed to get app version:", error);
        setVersion("");
      });
  });

  return (
    <div class="w-full border-t border-mid-gray/20 pt-3">
      <div class="flex flex-nowrap items-center gap-3 text-xs px-4 pb-3 text-text/60 whitespace-nowrap min-w-0">
        <ModelSelector />
        <span class="text-mid-gray/30">|</span>
        <BrainIndicator />
        <span class="text-mid-gray/30">|</span>
        <SystemMeters />

        <div class="flex shrink-0 items-center gap-1 ms-auto">
          <UpdateChecker />
          <span>•</span>
          <span>{version() ? `v${version()}` : null}</span>
        </div>
      </div>
    </div>
  );
}

export default Footer;
