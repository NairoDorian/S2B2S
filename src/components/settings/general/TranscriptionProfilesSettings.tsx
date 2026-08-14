import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { commands } from "@/bindings";
import type { TranscriptionProfile } from "@/bindings";
import { Plus, Trash2 } from "lucide-react";

const LANGUAGES: { value: string; label: string }[] = [
  { value: "auto", label: "Auto-detect" },
  { value: "os_input", label: "OS input source" },
  { value: "en", label: "English" },
  { value: "es", label: "Español" },
  { value: "fr", label: "Français" },
  { value: "de", label: "Deutsch" },
  { value: "it", label: "Italiano" },
  { value: "pt", label: "Português" },
  { value: "ru", label: "Русский" },
  { value: "ja", label: "日本語" },
  { value: "zh", label: "中文" },
  { value: "ko", label: "한국어" },
  { value: "ar", label: "العربية" },
  { value: "hi", label: "हिन्दी" },
  { value: "tr", label: "Türkçe" },
  { value: "nl", label: "Nederlands" },
  { value: "pl", label: "Polski" },
  { value: "sv", label: "Svenska" },
  { value: "vi", label: "Tiếng Việt" },
  { value: "uk", label: "Українська" },
];

/**
 * Named transcription profiles: one-click presets of STT model + language.
 * Activating a profile switches the active model and language immediately.
 */
export const TranscriptionProfilesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const { models } = useModelStore();
  const [profiles, setProfiles] = useState<TranscriptionProfile[]>([]);
  const [name, setName] = useState("");
  const [model, setModel] = useState<string | null>(null);
  const [language, setLanguage] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const result = await commands.getTranscriptionProfiles();
    if (result.status === "ok" && result.data) {
      setProfiles(result.data);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const activeId = settings?.active_transcription_profile_id ?? null;

  const addProfile = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const result = await commands.saveTranscriptionProfile({
      id: "",
      name: trimmed,
      model: model || null,
      language: language || null,
    });
    if (result.status === "ok" && result.data) {
      setProfiles(result.data);
      setName("");
      setModel(null);
      setLanguage(null);
    }
  };

  const activate = async (profileId: string) => {
    await commands.setActiveTranscriptionProfile(profileId);
  };

  const deactivate = async () => {
    await commands.setActiveTranscriptionProfile(null);
  };

  const remove = async (profileId: string) => {
    const result = await commands.deleteTranscriptionProfile(profileId);
    if (result.status === "ok" && result.data) {
      setProfiles(result.data);
    }
  };

  const modelOptions = (models ?? [])
    .filter((m) => m.is_downloaded)
    .map((m) => ({ value: m.id, label: m.name }));

  const modelLabel = (profileModel: string | null | undefined) =>
    profileModel
      ? (modelOptions.find((o) => o.value === profileModel)?.label ??
        profileModel)
      : t("settings.profiles.keepCurrent");

  const languageLabel = (profileLanguage: string | null | undefined) =>
    profileLanguage
      ? (LANGUAGES.find((l) => l.value === profileLanguage)?.label ??
        profileLanguage)
      : t("settings.profiles.keepCurrent");

  return (
    <SettingsGroup title={t("settings.profiles.group")}>
      {profiles.length === 0 ? (
        <p className="px-4 pb-2 text-xs text-text/50">
          {t("settings.profiles.noProfiles")}
        </p>
      ) : (
        <div className="divide-y divide-mid-gray/20">
          {profiles.map((profile) => {
            const isActive = profile.id === activeId;
            return (
              <div
                key={profile.id}
                className={`flex items-center justify-between gap-3 px-4 py-2.5 ${isActive ? "bg-logo-primary/5" : ""}`}
              >
                <div className="min-w-0">
                  <span className="flex items-center gap-2 text-sm font-medium text-text">
                    {profile.name}
                    {isActive && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-logo-primary/15 text-logo-primary font-bold">
                        {t("settings.profiles.activeBadge")}
                      </span>
                    )}
                  </span>
                  <span className="block text-[11px] text-text/40 truncate">
                    {modelLabel(profile.model)} ·{" "}
                    {languageLabel(profile.language)}
                  </span>
                </div>
                <div className="flex items-center gap-1.5 shrink-0">
                  {isActive ? (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void deactivate()}
                    >
                      {t("settings.profiles.deactivate")}
                    </Button>
                  ) : (
                    <Button
                      variant="primary-soft"
                      size="sm"
                      onClick={() => void activate(profile.id)}
                    >
                      {t("settings.profiles.activate")}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    aria-label={t("settings.profiles.delete")}
                    onClick={() => void remove(profile.id)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <SettingContainer
        title={t("settings.profiles.add")}
        description={t("settings.profiles.addDescription")}
        grouped
        layout="stacked"
      >
        <div className="flex flex-col gap-2">
          <Input
            variant="compact"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("settings.profiles.namePlaceholder")}
          />
          <div className="flex gap-2">
            <Select
              value={model}
              options={modelOptions}
              placeholder={t("settings.profiles.modelPlaceholder")}
              onChange={(value) => setModel(value)}
              className="flex-1 min-w-0"
            />
            <Select
              value={language}
              options={LANGUAGES}
              placeholder={t("settings.profiles.languagePlaceholder")}
              onChange={(value) => setLanguage(value)}
              className="flex-1 min-w-0"
            />
          </div>
          <div>
            <Button
              variant="secondary"
              size="sm"
              disabled={!name.trim()}
              onClick={() => void addProfile()}
            >
              <Plus size={14} className="mr-1" />
              {t("settings.profiles.addButton")}
            </Button>
          </div>
        </div>
      </SettingContainer>
    </SettingsGroup>
  );
};
