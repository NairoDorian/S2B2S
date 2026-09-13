import { createSignal, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands, type KeyboardDiagnosticReport } from "@/bindings";
import { useOsType } from "../../../hooks/useOsType";

export const KeyboardDiagnostic = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [running, setRunning] = createSignal(false);
  const [report, setReport] = createSignal<KeyboardDiagnosticReport | null>(
    null,
  );
  const [error, setError] = createSignal<string | null>(null);

  if (osType !== "macos") {
    return null;
  }

  const runDiagnostic = async () => {
    setRunning(true);
    setReport(null);
    setError(null);
    try {
      const result = await commands.runKeyboardDiagnostic(10);
      if (result.status === "ok") {
        setReport(result.data);
      } else {
        setError(result.error);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const verdict = (r: KeyboardDiagnosticReport): string => {
    if (r.secure_input_enabled && r.key_down === 0) {
      return t("settings.debug.keyboardDiagnostic.verdictBlocked");
    }
    if (!r.secure_input_enabled && r.key_down === 0 && r.flags_changed > 0) {
      return t("settings.debug.keyboardDiagnostic.verdictSuspicious");
    }
    if (r.key_down === 0 && r.flags_changed === 0 && r.mouse === 0) {
      return t("settings.debug.keyboardDiagnostic.verdictNoEvents");
    }
    return t("settings.debug.keyboardDiagnostic.verdictOk");
  };

  const secureInputLine = (r: KeyboardDiagnosticReport): string => {
    const state = r.secure_input_enabled
      ? t("settings.debug.keyboardDiagnostic.enabled")
      : t("settings.debug.keyboardDiagnostic.disabled");
    if (!r.secure_input_enabled) {
      return state;
    }
    const holder =
      r.culprit_name !== null
        ? t("settings.debug.keyboardDiagnostic.holder", {
            name: r.culprit_name,
            pid: r.culprit_pid,
          })
        : t("settings.debug.keyboardDiagnostic.holderUnknown");
    return `${state} — ${holder}`;
  };

  return (
    <div class="p-4 space-y-2">
      <div class="flex justify-between items-center gap-2">
        <div>
          <p class="text-sm font-medium">
            {t("settings.debug.keyboardDiagnostic.title")}
          </p>
          <p class="text-xs text-mid-gray">
            {t("settings.debug.keyboardDiagnostic.description")}
          </p>
        </div>
        <button
          onClick={runDiagnostic}
          disabled={running()}
          class="px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-accent/10 rounded cursor-pointer hover:border-accent disabled:opacity-50 disabled:cursor-default whitespace-nowrap"
        >
          {t("settings.debug.keyboardDiagnostic.run")}
        </button>
      </div>
      {running() && (
        <p class="text-sm animate-pulse">
          {t("settings.debug.keyboardDiagnostic.running")}
        </p>
      )}
      {error() !== null && (
        <p class="text-sm text-red-500">
          {t("settings.debug.keyboardDiagnostic.failed", { error: error() })}
        </p>
      )}
      <Show when={report()}>
        {(r) => (
          <div class="text-sm font-mono space-y-1">
            <p>
              {t("settings.debug.keyboardDiagnostic.secureInputLabel")}:{" "}
              {secureInputLine(r())}
            </p>
            <p>
              {t("settings.debug.keyboardDiagnostic.keyDown")}: {r().key_down} ·{" "}
              {t("settings.debug.keyboardDiagnostic.keyUp")}: {r().key_up} ·{" "}
              {t("settings.debug.keyboardDiagnostic.flagsChanged")}:{" "}
              {r().flags_changed} ·{" "}
              {t("settings.debug.keyboardDiagnostic.mouse")}: {r().mouse}
            </p>
            <p class="font-sans font-medium">{verdict(r())}</p>
          </div>
        )}
      </Show>
    </div>
  );
};
