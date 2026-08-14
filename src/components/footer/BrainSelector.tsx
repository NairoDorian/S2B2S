import React, { useState, useRef, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type LlamaServerStatus } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import appIcon from "../../assets/icon.png";

type BrainStatus = "disabled" | "loading" | "ready" | "stopped";

const BrainSelector: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, setBrainProvider } = useSettings();
  const [isOpen, setIsOpen] = useState(false);
  const [llamaServer, setLlamaServer] = useState<LlamaServerStatus | null>(
    null,
  );
  const dropdownRef = useRef<HTMLDivElement>(null);

  const brain = settings?.brain;

  // Keep the footer in sync with the llama.cpp server no matter which feature
  // started it (conversation, post-processing, multi-STT merge/Gemma 4 STT).
  useEffect(() => {
    const refresh = () => {
      void commands
        .getBrainServerStatus()
        .then((res) => {
          if (res.status === "ok") setLlamaServer(res.data);
        })
        .catch((err) => {
          console.error("Failed to fetch brain server status:", err);
        });
    };

    void refresh();
    // Poll as a safety net for state changes made outside this window
    // (e.g. the server gets killed externally or by another command).
    const interval = setInterval(refresh, 5000);

    const unlisten = events.llamaServerStatus.listen((event) => {
      setLlamaServer(event.payload);
    });

    return () => {
      clearInterval(interval);
      unlisten.then((fn) => fn());
    };
  }, []);

  const deriveStatus = useCallback((): BrainStatus => {
    if (!brain?.enabled) return "disabled";
    if (brain.provider_id !== "llama_cpp") return "ready";
    if (!llamaServer) return "loading";
    if (llamaServer.state === "loading") return "loading";
    if (llamaServer.running) return "ready";
    return "stopped";
  }, [brain?.enabled, brain?.provider_id, llamaServer]);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  if (!brain) return null;

  const status = deriveStatus();

  const activeProvider = brain.providers.find(
    (p) => p.id === brain.provider_id,
  );
  const rawModel = brain.models[brain.provider_id] || "";
  const providerLabel = activeProvider?.label || brain.provider_id;

  // Display-friendly model name
  const displayModel =
    brain.provider_id === "llama_cpp" ? "Gemma-4 2B (Local)" : rawModel;

  // Rich status line for the local server: model + mmproj + backend.
  const llamaDetails = (() => {
    if (brain.provider_id !== "llama_cpp") return null;
    const server = llamaServer;
    if (!server || !server.running) return null;
    const model = server.model ? server.model.split("/").pop() : "";
    const backendLabel = server.backend ? server.backend.toUpperCase() : "";
    const mmprojLabel = server.mmprojLoaded ? " + mmproj" : "";
    return `${model}${mmprojLabel}${backendLabel ? ` · ${backendLabel}` : ""}`;
  })();

  const tooltip = !brain.enabled
    ? "Brain Disabled"
    : status === "loading"
      ? `Brain: Loading llama.cpp model...${llamaDetails ? ` (${llamaDetails})` : ""}`
      : status === "stopped"
        ? "Brain: llama.cpp server not running"
        : `Brain: ${providerLabel}${llamaDetails ? ` (${llamaDetails})` : displayModel ? ` (${displayModel})` : ""}`;

  const handleToggleEnabled = async () => {
    await updateSetting("brain", {
      ...brain,
      enabled: !brain.enabled,
    });
  };

  const handleProviderSelect = async (providerId: string) => {
    if (providerId === brain.provider_id) return;
    await setBrainProvider(providerId);
    setIsOpen(false);
  };

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-1.5 hover:text-text/80 transition-colors cursor-pointer text-xs focus:outline-none"
        title={tooltip}
      >
        <span className="flex items-center gap-1.5">
          <img
            src={appIcon}
            alt="Brain"
            className="w-3.5 h-3.5 object-contain"
          />
          <span className="font-medium">{t("footer.brain")}</span>
        </span>
        <div
          className={`w-1.5 h-1.5 rounded-full transition-colors duration-300 ${
            status === "loading"
              ? "bg-orange-400 animate-pulse"
              : status === "ready"
                ? "bg-green-400"
                : status === "stopped"
                  ? "bg-yellow-500"
                  : "bg-mid-gray/40"
          }`}
        />
        <svg
          className={`w-3 h-3 transition-transform duration-200 ${isOpen ? "rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute bottom-full start-0 mb-2 w-64 max-h-[60vh] overflow-y-auto bg-background border border-mid-gray/20 rounded-lg shadow-lg py-2.5 px-3 z-50 text-xs">
          <div className="flex items-center justify-between pb-2 mb-2 border-b border-mid-gray/10">
            <div className="flex flex-col">
              <span className="font-semibold text-text/80">
                {t("footer.brainTitle")}
              </span>
              {brain.enabled && (
                <span className="text-[10px] text-text/50 font-normal truncate max-w-44">
                  {brain.provider_id === "llama_cpp"
                    ? llamaDetails || displayModel
                    : displayModel}
                </span>
              )}
              {brain.provider_id === "llama_cpp" && llamaServer?.running && (
                <span className="text-[10px] text-text/40 font-normal truncate max-w-44">
                  {llamaServer.backend.toUpperCase()}
                  {llamaServer.mmprojLoaded ? " · mmproj" : " · text-only"}
                </span>
              )}
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                checked={brain.enabled}
                onChange={handleToggleEnabled}
                className="sr-only peer"
              />
              <div className="w-7 h-4 bg-mid-gray/20 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-text/70 after:border-gray-300 after:border after:rounded-full after:h-3 after:w-3 after:transition-all peer-checked:bg-logo-primary peer-checked:after:bg-white"></div>
            </label>
          </div>

          <div className="space-y-1">
            <div className="text-[10px] text-text/40 uppercase font-medium tracking-wider mb-1 px-1">
              {t("footer.brainActiveProvider")}
            </div>
            {brain.providers.map((provider) => (
              <button
                key={provider.id}
                onClick={() => handleProviderSelect(provider.id)}
                className={`w-full px-2 py-1.5 rounded text-start flex items-center justify-between hover:bg-mid-gray/10 transition-colors cursor-pointer ${
                  brain.provider_id === provider.id
                    ? "bg-logo-primary/10 text-logo-primary font-medium"
                    : "text-text/70"
                }`}
              >
                <span>{provider.label}</span>
                {brain.provider_id === provider.id && (
                  <div className="w-1.5 h-1.5 rounded-full bg-logo-primary" />
                )}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default BrainSelector;
