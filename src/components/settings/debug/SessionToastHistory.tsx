import React, { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle, AlertTriangle, Filter } from "lucide-react";
import { useSessionToastStore } from "@/stores/sessionToastStore";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { SettingContainer } from "@/components/ui/SettingContainer";

export const SessionToastHistory: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { toasts, showErrors, showWarnings, setShowErrors, setShowWarnings } =
    useSessionToastStore();

  const errorCount = toasts.filter((t) => t.level === "error").length;
  const warningCount = toasts.filter((t) => t.level === "warning").length;

  const visibleToasts = useMemo(
    () =>
      toasts
        .filter(
          (t) =>
            (t.level === "error" && showErrors) ||
            (t.level === "warning" && showWarnings),
        )
        .reverse(),
    [toasts, showErrors, showWarnings],
  );

  const dateTimeFormatter = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.resolvedLanguage ?? i18n.language, {
        dateStyle: "short",
        timeStyle: "medium",
      }),
    [i18n.language, i18n.resolvedLanguage],
  );

  return (
    <SettingsGroup
      title={t("settings.debug.sessionToasts.title", { count: toasts.length })}
      description={t("settings.debug.sessionToasts.description")}
    >
      <div className="flex items-center gap-3 mb-4">
        <Filter className="h-4 w-4 text-[#777]" aria-hidden="true" />
        <SettingContainer
          title={t("settings.debug.sessionToasts.filters.errors", { count: errorCount })}
          description={t("settings.debug.sessionToasts.filters.errorsDescription")}
          descriptionMode="inline"
          grouped={true}
        >
          <ToggleSwitch
            checked={showErrors}
            onChange={setShowErrors}
            label={t("settings.debug.sessionToasts.filters.errors", { count: errorCount })}
            description={t("settings.debug.sessionToasts.filters.errorsDescription")}
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.debug.sessionToasts.filters.warnings", { count: warningCount })}
          description={t("settings.debug.sessionToasts.filters.warningsDescription")}
          descriptionMode="inline"
          grouped={true}
        >
          <ToggleSwitch
            checked={showWarnings}
            onChange={setShowWarnings}
            label={t("settings.debug.sessionToasts.filters.warnings", { count: warningCount })}
            description={t("settings.debug.sessionToasts.filters.warningsDescription")}
          />
        </SettingContainer>
      </div>

      {toasts.length === 0 ? (
        <div className="px-6 py-5 text-sm text-[#a0a0a0]">
          {t("settings.debug.sessionToasts.empty")}
        </div>
      ) : visibleToasts.length === 0 ? (
        <div className="px-6 py-5 text-sm text-[#a0a0a0]">
          {t("settings.debug.sessionToasts.filteredEmpty", { errorCount, warningCount })}
        </div>
      ) : (
        <div className="divide-y divide-white/[0.05]">
          {visibleToasts.map((toast) => {
            const isError = toast.level === "error";
            const Icon = isError ? AlertCircle : AlertTriangle;

            return (
              <article key={toast.id} className="px-6 py-4">
                <div className="flex items-start gap-3">
                  <Icon
                    className={`mt-0.5 h-4 w-4 shrink-0 ${
                      isError ? "text-red-400" : "text-yellow-400"
                    }`}
                    aria-hidden="true"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="mb-1.5 flex flex-wrap items-center gap-x-2 gap-y-1">
                      <span
                        className={`text-[10px] font-semibold uppercase tracking-wider ${
                          isError ? "text-red-300" : "text-yellow-300"
                        }`}
                      >
                        {t(`settings.debug.sessionToasts.levels.${toast.level}`)}
                      </span>
                      <time
                        className="text-[11px] text-[#777]"
                        dateTime={new Date(toast.shownAt).toISOString()}
                      >
                        {dateTimeFormatter.format(toast.shownAt)}
                      </time>
                    </div>
                    {toast.message && (
                      <p className="text-sm text-white break-words">{toast.message}</p>
                    )}
                    {toast.description && (
                      <p className="mt-1.5 text-sm text-[#aaa]">{toast.description}</p>
                    )}
                    {toast.actionLabel && (
                      <span className="mt-2 inline-flex items-center gap-1.5 text-xs text-[#888]">
                        <span className="px-1.5 py-0.5 rounded bg-white/5">{toast.actionLabel}</span>
                        {t("settings.debug.sessionToasts.actionExecuted")}
                      </span>
                    )}
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </SettingsGroup>
  );
};