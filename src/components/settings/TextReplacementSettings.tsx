import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../ui/SettingsGroup";
import { SettingContainer } from "../ui/SettingContainer";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { commands } from "@/bindings";
import type { TextReplacement } from "@/bindings";
import { Plus, Trash2 } from "lucide-react";

/**
 * Text replacement rules applied after STT/ITN: expansions ("omw" →
 * "on my way"), abbreviations, or regex rewrites, with case-sensitivity and
 * escape sequences (\n, \t, \\, \u{...}).
 */
export const TextReplacementSettings: React.FC = () => {
  const { t } = useTranslation();
  const [rules, setRules] = useState<TextReplacement[]>([]);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [isRegex, setIsRegex] = useState(false);

  const refresh = useCallback(async () => {
    const result = await commands.getAppSettings();
    if (result.status === "ok") {
      setRules(result.data.text_replacements ?? []);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const addRule = async () => {
    const trimmed = from.trim();
    if (!trimmed) return;
    const result = await commands.saveTextReplacement({
      id: "",
      from: trimmed,
      to,
      enabled: true,
      case_sensitive: caseSensitive,
      is_regex: isRegex,
    });
    if (result.status === "ok") {
      setRules(result.data);
      setFrom("");
      setTo("");
      setCaseSensitive(false);
      setIsRegex(false);
    }
  };

  const toggle = async (rule: TextReplacement, enabled: boolean) => {
    const result = await commands.saveTextReplacement({ ...rule, enabled });
    if (result.status === "ok") {
      setRules(result.data);
    }
  };

  const remove = async (ruleId: string) => {
    const result = await commands.deleteTextReplacement(ruleId);
    if (result.status === "ok") {
      setRules(result.data);
    }
  };

  return (
    <SettingsGroup title={t("settings.textReplacement.group")}>
      {rules.length === 0 ? (
        <p className="px-4 pb-2 text-xs text-text/50">
          {t("settings.textReplacement.empty")}
        </p>
      ) : (
        <div className="divide-y divide-mid-gray/20">
          {rules.map((rule) => (
            <div
              key={rule.id}
              className="flex items-center justify-between gap-3 px-4 py-2"
            >
              <ToggleSwitch
                checked={rule.enabled}
                onChange={(enabled) => void toggle(rule, enabled)}
                label={
                  rule.is_regex
                    ? `/${rule.from}/ → ${rule.to}`
                    : `${rule.from} → ${rule.to}`
                }
                description={[
                  rule.case_sensitive
                    ? t("settings.textReplacement.caseSensitive")
                    : t("settings.textReplacement.caseInsensitive"),
                  rule.is_regex ? t("settings.textReplacement.regex") : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
                grouped
              />
              <Button
                variant="ghost"
                size="sm"
                aria-label={t("settings.textReplacement.delete")}
                onClick={() => void remove(rule.id)}
              >
                <Trash2 size={14} />
              </Button>
            </div>
          ))}
        </div>
      )}

      <SettingContainer
        title={t("settings.textReplacement.add")}
        description={t("settings.textReplacement.addDescription")}
        grouped
        layout="stacked"
      >
        <div className="flex flex-col gap-2">
          <div className="flex gap-2">
            <Input
              variant="compact"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              placeholder={t("settings.textReplacement.fromPlaceholder")}
              className="flex-1 min-w-0"
            />
            <Input
              variant="compact"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              placeholder={t("settings.textReplacement.toPlaceholder")}
              className="flex-1 min-w-0"
            />
          </div>
          <div className="flex items-center gap-4 text-xs text-text/60">
            <label className="inline-flex items-center gap-1.5 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={caseSensitive}
                onChange={(e) => setCaseSensitive(e.target.checked)}
                className="w-3.5 h-3.5 rounded border-mid-gray/40 text-logo-primary focus:ring-logo-primary bg-background-ui"
              />
              {t("settings.textReplacement.caseSensitive")}
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={isRegex}
                onChange={(e) => setIsRegex(e.target.checked)}
                className="w-3.5 h-3.5 rounded border-mid-gray/40 text-logo-primary focus:ring-logo-primary bg-background-ui"
              />
              {t("settings.textReplacement.regex")}
            </label>
          </div>
          <div>
            <Button
              variant="secondary"
              size="sm"
              disabled={!from.trim()}
              onClick={() => void addRule()}
            >
              <Plus size={14} className="mr-1" />
              {t("settings.textReplacement.addButton")}
            </Button>
          </div>
        </div>
      </SettingContainer>
    </SettingsGroup>
  );
};
