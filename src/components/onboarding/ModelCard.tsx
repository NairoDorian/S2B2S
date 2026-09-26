/* oxlint-disable jsx-a11y/no-static-element-interactions */
import { useTranslation } from "@/i18n/useTranslation";
import {
  AudioLines,
  Check,
  Download,
  Globe,
  HardDrive,
  Languages,
  Loader2,
  Trash2,
} from "@/components/icons/lucide";
import type { ModelInfo } from "@/bindings";
import { formatModelSize } from "../../lib/utils/format";
import {
  getTranslatedModelDescription,
  getTranslatedModelName,
} from "../../lib/utils/modelTranslation";
import {
  getLanguageLabel,
  getUniqueCapabilityLanguages,
} from "../../lib/constants/languages";
import Badge from "../ui/Badge";
import { Button } from "../ui/Button";
import { useSettingsStore } from "@/stores/settingsStore";

const getLanguageDisplayText = (
  supportedLanguages: string[],
  t: (key: string, options?: Record<string, unknown>) => string,
): string => {
  const capabilityLanguages = getUniqueCapabilityLanguages(supportedLanguages);
  if (capabilityLanguages.length === 1) {
    const langCode = capabilityLanguages[0];
    const langName = getLanguageLabel(langCode) || langCode;
    return t("modelSelector.capabilities.languageOnly", { language: langName });
  }
  return t("modelSelector.capabilities.languageCount", {
    total: capabilityLanguages.length,
  });
};

export const isLegacySource = (model: ModelInfo): boolean =>
  typeof model.source === "object" && "Url" in model.source;

const getQuantLabel = (filename: string): string | null => {
  const match = filename.match(
    /[._-](TQ\d+_\w+|IQ\d+_\w+|Q\d+(?:_\w+)?|F16|BF16|F32)\.gguf$/i,
  );
  return match ? match[1].toUpperCase() : null;
};

export type ModelCardStatus =
  | "downloadable"
  | "downloading"
  | "verifying"
  | "switching"
  | "active"
  | "available";

interface ModelCardProps {
  model: ModelInfo;
  variant?: "default" | "featured";
  status?: ModelCardStatus;
  disabled?: boolean;
  class?: string;
  onSelect: (modelId: string) => void;
  onDownload?: (modelId: string) => void;
  onDelete?: (modelId: string) => void;
  onCancel?: (modelId: string) => void;
  downloadProgress?: number;
  downloadSpeed?: number;
  showRecommended?: boolean;
}

const ModelCard = (props: ModelCardProps) => {
  // Props are read through accessors, never destructured: the lists render
  // cards keyed by model id, so one card instance lives through its model's
  // whole download → verify → available → active lifecycle.
  const { t } = useTranslation();
  const settingsStore = useSettingsStore();
  const debugMode = () => settingsStore.settings?.debug_mode ?? false;
  const status = () => props.status ?? "downloadable";
  const disabled = () => props.disabled ?? false;
  const isClickable = () =>
    status() === "available" || status() === "downloadable";

  const displayName = () => getTranslatedModelName(props.model, t);
  const displayDescription = () =>
    getTranslatedModelDescription(props.model, t);
  const showModelSize = () =>
    status() === "downloadable" ||
    status() === "available" ||
    status() === "active";
  const formattedModelSize = () => formatModelSize(Number(props.model.size_mb));
  const quantLabel = () => getQuantLabel(props.model.filename);
  const capabilityLanguages = () =>
    getUniqueCapabilityLanguages(props.model.supported_languages);

  const baseClasses =
    "flex flex-col rounded-xl px-4 py-3 gap-2 text-left transition-all duration-200";

  const getVariantClasses = () => {
    if (status() === "active") {
      return "border-2 border-accent/50 bg-accent/10";
    }
    if (props.variant === "featured") {
      return "border-2 border-accent/25 bg-accent/5";
    }
    return "border-2 border-mid-gray/20";
  };

  const getInteractiveClasses = () => {
    if (!isClickable()) return "";
    if (disabled()) return "opacity-50 cursor-not-allowed";
    return "cursor-pointer hover:border-accent/50 hover:bg-accent/5 hover:shadow-lg hover:scale-[1.01] active:scale-[0.99] group";
  };

  const handleClick = () => {
    if (!isClickable() || disabled()) return;
    if (status() === "downloadable" && props.onDownload) {
      props.onDownload(props.model.id);
    } else {
      props.onSelect(props.model.id);
    }
  };

  const handleDelete = (e: MouseEvent) => {
    e.stopPropagation();
    props.onDelete?.(props.model.id);
  };

  // ModelCard is itself interactive only while it can be picked or downloaded
  // (`available` / `downloadable`: a click selects or starts the download);
  // inner buttons handle secondary actions.
  return (
    <div
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" && isClickable()) handleClick();
      }}
      role={isClickable() ? "button" : undefined}
      tabindex={isClickable() ? 0 : undefined}
      class={[
        baseClasses,
        getVariantClasses(),
        getInteractiveClasses(),
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div class="flex justify-between items-center w-full">
        <div class="flex flex-col items-start flex-1 min-w-0">
          <div class="flex items-center gap-3 flex-wrap">
            <h3
              class={`text-base font-semibold text-text ${isClickable() ? "group-hover:text-accent" : ""} transition-colors`}
            >
              {displayName()}
            </h3>
            {(props.showRecommended ?? true) && props.model.is_recommended && (
              <Badge variant="primary">{t("onboarding.recommended")}</Badge>
            )}
            {status() === "active" && (
              <Badge variant="primary">
                <Check class="w-3 h-3 mr-1" />
                {t("modelSelector.active")}
              </Badge>
            )}
            {props.model.is_custom && (
              <Badge variant="secondary">{t("modelSelector.custom")}</Badge>
            )}
            {isLegacySource(props.model) && (
              <Badge variant="secondary">{t("modelSelector.legacy")}</Badge>
            )}
            {status() === "switching" && (
              <Badge variant="secondary">
                <Loader2 class="w-3 h-3 mr-1 animate-spin" />
                {t("modelSelector.switching")}
              </Badge>
            )}
          </div>
          <p class="text-text/60 text-sm leading-relaxed">
            {displayDescription()}
          </p>
        </div>
        {(props.model.accuracy_score! > 0 || props.model.speed_score! > 0) && (
          <div class="hidden sm:flex items-center ms-4">
            <div class="space-y-1">
              <div class="flex items-center gap-2">
                <p class="text-xs text-text/60 w-24 text-end">
                  {t("onboarding.modelCard.accuracy")}
                </p>
                <div class="w-16 h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
                  <div
                    class="h-full bg-accent rounded-full"
                    style={{ width: `${props.model.accuracy_score! * 100}%` }}
                  />
                </div>
              </div>
              <div class="flex items-center gap-2">
                <p class="text-xs text-text/60 w-24 text-end">
                  {t("onboarding.modelCard.speed")}
                </p>
                <div class="w-16 h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
                  <div
                    class="h-full bg-accent rounded-full"
                    style={{ width: `${props.model.speed_score! * 100}%` }}
                  />
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      <hr class="w-full border-mid-gray/20" />

      <div class="flex items-center gap-3 w-full -mb-0.5 mt-0.5 h-5">
        {capabilityLanguages().length > 0 && (
          <div
            class="flex items-center gap-1 text-xs text-text/50"
            title={
              capabilityLanguages().length === 1
                ? t("modelSelector.capabilities.singleLanguage")
                : t("modelSelector.capabilities.languageSelection")
            }
          >
            <Globe class="w-3.5 h-3.5" />
            <span>
              {getLanguageDisplayText(props.model.supported_languages, t)}
            </span>
          </div>
        )}
        {props.model.supports_translation && (
          <div
            class="flex items-center gap-1 text-xs text-text/50"
            title={t("modelSelector.capabilities.translation")}
          >
            <Languages class="w-3.5 h-3.5" />
            <span>{t("modelSelector.capabilities.translate")}</span>
          </div>
        )}
        {props.model.supports_streaming && (
          <div
            class="flex items-center gap-1 text-xs text-text/50"
            title={t("modelSelector.capabilities.streaming")}
          >
            <AudioLines class="w-3.5 h-3.5" />
            <span>{t("modelSelector.streaming")}</span>
          </div>
        )}
        {showModelSize() && (
          <span class="flex items-center gap-1.5 ms-auto text-xs text-text/50">
            {status() === "downloadable" ? (
              <Download class="w-3.5 h-3.5" />
            ) : (
              <HardDrive class="w-3.5 h-3.5" />
            )}
            <span>{formattedModelSize()}</span>
            {debugMode() && quantLabel() && (
              <span class="text-text/40">{quantLabel()}</span>
            )}
          </span>
        )}
        {props.onDelete &&
          (status() === "available" || status() === "active") && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleDelete}
              title={t("modelSelector.deleteModel", {
                modelName: displayName(),
              })}
              class="flex items-center gap-1.5 text-accent/85 hover:text-accent hover:bg-accent/10"
            >
              <Trash2 class="w-3.5 h-3.5" />
              <span>{t("common.delete")}</span>
            </Button>
          )}
      </div>

      {status() === "downloading" && props.downloadProgress !== undefined && (
        <div class="w-full mt-3">
          <div class="w-full h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
            <div
              class="h-full bg-accent rounded-full transition-all duration-300"
              style={{ width: `${props.downloadProgress}%` }}
            />
          </div>
          <div class="flex items-center justify-between text-xs mt-1">
            <span class="text-text/50">
              {t("modelSelector.downloading", {
                percentage: Math.round(props.downloadProgress ?? 0),
              })}
            </span>
            <div class="flex items-center gap-2">
              {props.downloadSpeed !== undefined && props.downloadSpeed > 0 && (
                <span class="tabular-nums text-text/50">
                  {t("common.downloadSpeed", {
                    speed: (props.downloadSpeed ?? 0).toFixed(1),
                  })}
                </span>
              )}
              {props.onCancel && (
                <Button
                  variant="danger-ghost"
                  size="sm"
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    props.onCancel?.(props.model.id);
                  }}
                  aria-label={t("modelSelector.cancelDownload")}
                >
                  {t("modelSelector.cancel")}
                </Button>
              )}
            </div>
          </div>
        </div>
      )}
      {status() === "verifying" && (
        <div class="w-full mt-3">
          <div class="w-full h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
            <div class="h-full bg-accent rounded-full animate-pulse w-full" />
          </div>
          <p class="text-xs text-text/50 mt-1">
            {t("modelSelector.verifyingGeneric")}
          </p>
        </div>
      )}
    </div>
  );
};

export default ModelCard;
