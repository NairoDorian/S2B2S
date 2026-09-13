import { useTranslation } from "@/i18n/useTranslation";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface LazyStreamCloseProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const LazyStreamClose = (props: LazyStreamCloseProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("lazy_stream_close") ?? false}
      onChange={(enabled) => updateSetting("lazy_stream_close", enabled)}
      isUpdating={isUpdating("lazy_stream_close")}
      label={t("settings.advanced.lazyStreamClose.label")}
      description={t("settings.advanced.lazyStreamClose.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
    />
  );
};
