import { createSignal, createEffect, onCleanup } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, TriangleAlert, X } from "@/components/icons/lucide";
import { commands, type SecureInputStatus } from "@/bindings";
import { REPO_URL } from "@/lib/appIdentity";

export const SECURE_INPUT_HELP_URL = `${REPO_URL}#troubleshooting`;

const SecureInputWarning = () => {
  const { t } = useTranslation();
  const [status, setStatus] = createSignal<SecureInputStatus | null>(null);
  const [dismissed, setDismissed] = createSignal(false);

  const refresh = async () => {
    try {
      setStatus(await commands.getSecureInputStatus());
    } catch (e) {
      console.warn("Failed to fetch secure input status:", e);
    }
  };

  createEffect(
    () => undefined,
    () => {
      refresh();
      listen<SecureInputStatus>("secure-input-changed", (event) =>
        setStatus(event.payload),
      ).then((unlisten) => {
        onCleanup(() => {
          const fn = unlisten as () => Promise<void>;
          fn().catch(() => {});
        });
      });
    },
  );

  const impacted = () =>
    status() !== null &&
    ((status()!.sustained &&
      (status()!.degraded_bindings.length > 0 ||
        status()!.uncovered_bindings.length > 0)) ||
      status()!.recorder_blocked);

  createEffect(
    () => undefined,
    () => {
      if (!impacted()) {
        setDismissed(false);
      }
    },
  );

  if (!impacted() || dismissed()) {
    return null;
  }

  const statusVal = status()!;
  const affectedCount = new Set([
    ...statusVal.uncovered_bindings,
    ...statusVal.degraded_bindings,
  ]).size;
  const countSuffix = affectedCount === 1 ? "one" : "other";
  const message =
    affectedCount > 0
      ? statusVal.culprit_name !== null
        ? t(`secureInput.blockedWithCulprit_${countSuffix}`, {
            name: statusVal.culprit_name,
            count: affectedCount,
          })
        : t(`secureInput.blockedNoCulprit_${countSuffix}`, {
            count: affectedCount,
          })
      : statusVal.culprit_name !== null
        ? t("secureInput.recorderBlockedWithCulprit", {
            name: statusVal.culprit_name,
          })
        : t("secureInput.recorderBlockedNoCulprit");

  return (
    <div class="w-full rounded-lg border border-warning/40 bg-warning/10 px-3 py-2.5">
      <div class="flex items-center gap-3">
        <TriangleAlert class="h-5 w-5 shrink-0 text-warning" />
        <p class="min-w-0 flex-1 text-sm font-medium leading-5">{message}</p>
        <div class="flex shrink-0 items-center gap-1">
          <button
            onClick={() => openUrl(SECURE_INPUT_HELP_URL)}
            class="cursor-pointer whitespace-nowrap rounded px-2 py-1.5 text-sm font-medium text-text hover:text-warning focus:outline-none focus:ring-1 focus:ring-warning"
          >
            <span class="flex items-center gap-1 border-b border-current leading-4">
              {t("secureInput.learnMore")}
              <ExternalLink class="h-3.5 w-3.5" />
            </span>
          </button>
          <button
            onClick={() => setDismissed(true)}
            aria-label={t("secureInput.dismiss")}
            class="cursor-pointer rounded p-1.5 text-mid-gray hover:bg-warning/15 hover:text-warning focus:outline-none focus:ring-1 focus:ring-warning"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
      </div>
    </div>
  );
};

export default SecureInputWarning;
