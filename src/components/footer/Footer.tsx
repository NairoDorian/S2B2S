import { createSignal, onSettled } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";

import ModelSelector from "../model-selector";
import UpdateChecker from "../update-checker";
import BrainIndicator from "./BrainIndicator";
import SystemMeters from "./SystemMeters";

function Footer() {
  const [version, setVersion] = createSignal("");

  onSettled(() => {
    const fetchVersion = async () => {
      try {
        const appVersion = await getVersion();
        setVersion(appVersion);
      } catch (error) {
        console.error("Failed to get app version:", error);
        setVersion("0.1.2");
      }
    };

    fetchVersion();
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
          <span>{`v${version()}`}</span>
        </div>
      </div>
    </div>
  );
}

export default Footer;
