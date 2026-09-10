import React, { useMemo } from "react";
import { AlertCircle, AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSessionToastStore } from "@/stores/sessionToastStore";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/**
 * Debug page: every error/warning toast shown this session, newest first,
 * with per-level filters. A toast that auto-dismissed while the user was
 * looking elsewhere can be read (and copied) here. Ported from AIVORelay.
 */
export const SessionToastHistory: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { toasts, showErrors, showWarnings, setShowErrors, setShowWarnings } =
    useSessionToastStore();

  const errorCount = toasts.filter((toast) => toast.level === "error").length;
  const warningCount = toasts.length - errorCount;
  const visibleToasts = useMemo(
    () =>
      toasts
        .filter(
          (toast) =>
            (toast.level === "error" && showErrors) ||
            (toast.level === "warning" && showWarnings),
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
      <ToggleSwitch
        checked={showErrors}
        onChange={setShowErrors}
        label={t("settings.debug.sessionToasts.filters.errors", {
          count: errorCount,
        })}
        description={t(
          "settings.debug.sessionToasts.filters.errorsDescription",
        )}
        grouped={true}
      />
      <ToggleSwitch
        checked={showWarnings}
        onChange={setShowWarnings}
        label={t("settings.debug.sessionToasts.filters.warnings", {
          count: warningCount,
        })}
        description={t(
          "settings.debug.sessionToasts.filters.warningsDescription",
        )}
        grouped={true}
      />

      {toasts.length === 0 ? (
        <div className="px-4 py-4 text-sm text-text/60">
          {t("settings.debug.sessionToasts.empty")}
        </div>
      ) : visibleToasts.length === 0 ? (
        <div className="px-4 py-4 text-sm text-text/60">
          {t("settings.debug.sessionToasts.filteredEmpty")}
        </div>
      ) : (
        <div className="divide-y divide-mid-gray/20 max-h-96 overflow-y-auto">
          {visibleToasts.map((toast) => {
            const isError = toast.level === "error";
            const Icon = isError ? AlertCircle : AlertTriangle;
            return (
              <article key={toast.id} className="px-4 py-3">
                <div className="flex items-start gap-3">
                  <Icon
                    className={`mt-0.5 h-4 w-4 shrink-0 ${
                      isError ? "text-red-400" : "text-warning"
                    }`}
                    aria-hidden="true"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="mb-1 flex flex-wrap items-center gap-x-2 gap-y-1">
                      <span
                        className={`text-[10px] font-semibold uppercase tracking-wider ${
                          isError ? "text-red-400" : "text-warning"
                        }`}
                      >
                        {t(
                          `settings.debug.sessionToasts.levels.${toast.level}`,
                        )}
                      </span>
                      <time
                        className="text-[11px] text-text/50"
                        dateTime={new Date(toast.shownAt).toISOString()}
                      >
                        {dateTimeFormatter.format(toast.shownAt)}
                      </time>
                    </div>
                    {toast.message && (
                      <p className="whitespace-pre-wrap break-words text-sm font-medium text-text select-text cursor-text">
                        {toast.message}
                      </p>
                    )}
                    {toast.description && (
                      <p className="mt-1 whitespace-pre-wrap break-words text-sm leading-relaxed text-text/70 select-text cursor-text">
                        {toast.description}
                      </p>
                    )}
                    {toast.actionLabel && (
                      <p className="mt-1.5 break-words text-xs text-text/50 select-text">
                        {t("settings.debug.sessionToasts.action", {
                          label: toast.actionLabel,
                        })}
                      </p>
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
