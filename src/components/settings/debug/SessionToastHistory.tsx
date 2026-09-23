import { createMemo, For } from "solid-js";
import { AlertCircle, AlertTriangle } from "@/components/icons/lucide";
import { currentLanguage, useTranslation } from "@/i18n/useTranslation";
import {
  setShowErrors,
  setShowWarnings,
  useSessionToastStore,
} from "@/stores/sessionToastStore";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/**
 * Debug page: every error/warning toast shown this session, newest first,
 * with per-level filters. A toast that auto-dismissed while the user was
 * looking elsewhere can be read (and copied) here. Ported from AIVORelay.
 */
export const SessionToastHistory = () => {
  const { t, i18n } = useTranslation();
  const store = useSessionToastStore();

  const errorCount = () =>
    store.toasts.filter((toast) => toast.level === "error").length;
  const warningCount = () => store.toasts.length - errorCount();
  const visibleToasts = createMemo(() =>
    store.toasts
      .filter(
        (toast) =>
          (toast.level === "error" && store.showErrors) ||
          (toast.level === "warning" && store.showWarnings),
      )
      .toReversed(),
  );

  // `currentLanguage()` is the reactive read that re-creates the formatter on
  // a language change; `i18n.resolvedLanguage` is a plain property.
  const dateTimeFormatter = createMemo(() => {
    const language = currentLanguage();
    return new Intl.DateTimeFormat(i18n.resolvedLanguage ?? language, {
      dateStyle: "short",
      timeStyle: "medium",
    });
  });

  return (
    <SettingsGroup
      title={t("settings.debug.sessionToasts.title", {
        count: store.toasts.length,
      })}
      description={t("settings.debug.sessionToasts.description")}
    >
      <ToggleSwitch
        checked={store.showErrors}
        onChange={setShowErrors}
        label={t("settings.debug.sessionToasts.filters.errors", {
          count: errorCount(),
        })}
        description={t(
          "settings.debug.sessionToasts.filters.errorsDescription",
        )}
        grouped={true}
      />
      <ToggleSwitch
        checked={store.showWarnings}
        onChange={setShowWarnings}
        label={t("settings.debug.sessionToasts.filters.warnings", {
          count: warningCount(),
        })}
        description={t(
          "settings.debug.sessionToasts.filters.warningsDescription",
        )}
        grouped={true}
      />

      {store.toasts.length === 0 ? (
        <div class="px-4 py-4 text-sm text-text/60">
          {t("settings.debug.sessionToasts.empty")}
        </div>
      ) : visibleToasts().length === 0 ? (
        <div class="px-4 py-4 text-sm text-text/60">
          {t("settings.debug.sessionToasts.filteredEmpty")}
        </div>
      ) : (
        <div class="divide-y divide-mid-gray/20 max-h-96 overflow-y-auto">
          <For each={visibleToasts()}>
            {(toast) => {
              const isError = toast.level === "error";
              const Icon = isError ? AlertCircle : AlertTriangle;
              return (
                <article class="px-4 py-3">
                  <div class="flex items-start gap-3">
                    <Icon
                      class={`mt-0.5 h-4 w-4 shrink-0 ${
                        isError ? "text-red-400" : "text-warning"
                      }`}
                      aria-hidden="true"
                    />
                    <div class="min-w-0 flex-1">
                      <div class="mb-1 flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span
                          class={`text-[10px] font-semibold uppercase tracking-wider ${
                            isError ? "text-red-400" : "text-warning"
                          }`}
                        >
                          {t(
                            `settings.debug.sessionToasts.levels.${toast.level}`,
                          )}
                        </span>
                        <time
                          class="text-[11px] text-text/50"
                          datetime={new Date(toast.shownAt).toISOString()}
                        >
                          {dateTimeFormatter().format(toast.shownAt)}
                        </time>
                      </div>
                      {toast.message && (
                        <p class="whitespace-pre-wrap break-words text-sm font-medium text-text select-text cursor-text">
                          {toast.message}
                        </p>
                      )}
                      {toast.description && (
                        <p class="mt-1 whitespace-pre-wrap break-words text-sm leading-relaxed text-text/70 select-text cursor-text">
                          {toast.description}
                        </p>
                      )}
                      {toast.actionLabel && (
                        <p class="mt-1.5 break-words text-xs text-text/50 select-text">
                          {t("settings.debug.sessionToasts.action", {
                            label: toast.actionLabel,
                          })}
                        </p>
                      )}
                    </div>
                  </div>
                </article>
              );
            }}
          </For>
        </div>
      )}
    </SettingsGroup>
  );
};
