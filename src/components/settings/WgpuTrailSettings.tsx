import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

const WgpuTrailSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const cfg = getSetting("wgpu_trail") || {};
  const enabled = cfg.enabled ?? false;
  const clickRipple = cfg.click_ripple ?? false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.advanced.wgpuTrail.groups.general")}>
        <ToggleSwitch
          checked={enabled}
          onChange={async (v) => {
            await updateSetting("wgpu_trail", { ...cfg, enabled: v });
          }}
          isUpdating={isUpdating("wgpu_trail")}
          label={t("settings.advanced.wgpuTrail.enabled.label")}
          description={t("settings.advanced.wgpuTrail.enabled.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        <ToggleSwitch
          checked={clickRipple}
          onChange={async (v) => {
            await updateSetting("wgpu_trail", { ...cfg, click_ripple: v });
          }}
          isUpdating={isUpdating("wgpu_trail")}
          label={t("settings.advanced.wgpuTrail.clickRipple.label")}
          description={t("settings.advanced.wgpuTrail.clickRipple.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
    </div>
  );
};

export default WgpuTrailSettings;
