import React, { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";

const BrainSelector: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, setBrainProvider } = useSettings();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const brain = settings?.brain;

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

  const activeProvider = brain.providers.find((p) => p.id === brain.provider_id);
  const activeModel = brain.models[brain.provider_id] || "";
  const providerLabel = activeProvider?.label || brain.provider_id;

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
        className="flex items-center gap-2 hover:text-text/80 transition-colors cursor-pointer text-xs focus:outline-none"
        title={brain.enabled ? `Brain enabled: ${providerLabel} (${activeModel})` : "Brain disabled"}
      >
        <div
          className={`w-2 h-2 rounded-full transition-colors duration-300 ${
            brain.enabled ? "bg-green-400" : "bg-mid-gray/40"
          }`}
        />
        <span className="max-w-40 truncate">
          {brain.enabled ? `${providerLabel}${activeModel ? ` (${activeModel})` : ""}` : t("settings.brain.disabled", "Brain Off")}
        </span>
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
            <span className="font-semibold text-text/80">AI Brain</span>
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
              Active Provider
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
