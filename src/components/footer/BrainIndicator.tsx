import { createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { BrainCircuit } from "@/components/icons/lucide";
import {
  useLlamaStore,
  initialize as initializeLlama,
} from "../../stores/llamaStore";
import { setSection } from "../../stores/navigationStore";

function BrainIndicator() {
  const { t } = useTranslation();
  const store = useLlamaStore();

  createEffect(
    () => undefined,
    () => {
      void initializeLlama();
    },
  );

  // Live accessors over the store: a body-level `const state = store.state`
  // would snapshot at mount and the dot would never leave "stopped".
  const state = () => store.state;
  const status = () => state()?.status ?? "stopped";
  const dot = () =>
    status() === "ready"
      ? "bg-emerald-400"
      : status() === "starting"
        ? "bg-warning animate-pulse"
        : status() === "error"
          ? "bg-error"
          : "bg-mid-gray/60";

  return (
    <button
      type="button"
      onClick={() => setSection("llama")}
      title={`${t("footer.brain")}: ${t(`settings.llama.status.${status()}`)}${state()?.message ? `\n${state()!.message}` : ""}`}
      class="flex items-center gap-1.5 shrink-0 hover:text-text/90 transition-colors cursor-pointer"
    >
      <BrainCircuit class="w-3.5 h-3.5 text-accent" />
      <span class={`w-2 h-2 ${dot()}`} />
      <span class="max-w-24 truncate font-mono">
        {status() === "ready"
          ? (state()?.alias ?? "")
          : t(`settings.llama.status.${status()}`)}
      </span>
    </button>
  );
}

export default BrainIndicator;
