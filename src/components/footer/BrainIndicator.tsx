import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { BrainCircuit } from "lucide-react";
import { useLlamaStore } from "../../stores/llamaStore";
import { useNavigationStore } from "../../stores/navigationStore";

/**
 * Status-bar "brain" indicator: the supervised llama.cpp server's state and
 * model alias. Click opens the Local LLM page. Driven by the state event the
 * backend pushes; no polling.
 */
export const BrainIndicator: React.FC = () => {
  const { t } = useTranslation();
  const state = useLlamaStore((s) => s.state);
  const initialize = useLlamaStore((s) => s.initialize);
  const setSection = useNavigationStore((s) => s.setSection);

  useEffect(() => {
    void initialize();
  }, [initialize]);

  const status = state?.status ?? "stopped";
  const dot =
    status === "ready"
      ? "bg-emerald-400"
      : status === "starting"
        ? "bg-warning animate-pulse"
        : status === "error"
          ? "bg-error"
          : "bg-mid-gray/60";
  const label = t(`settings.llama.status.${status}`);
  const detail = state?.alias ?? "";

  return (
    <button
      type="button"
      onClick={() => setSection("llama")}
      title={`${t("footer.brain")}: ${label}${state?.message ? `\n${state.message}` : ""}`}
      className="flex items-center gap-1.5 shrink-0 hover:text-text/90 transition-colors cursor-pointer"
    >
      <BrainCircuit className="w-3.5 h-3.5 text-logo-primary" />
      <span className={`w-2 h-2 ${dot}`} />
      <span className="max-w-32 truncate font-mono">
        {status === "ready" ? detail : label}
      </span>
    </button>
  );
};

export default BrainIndicator;
