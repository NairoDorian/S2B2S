import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import type { TypingTool } from "@/bindings";

interface TypingToolProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const allToolLabels: Record<string, string> = {
  wtype: "wtype",
  kwtype: "kwtype",
  dotool: "dotool",
  ydotool: "ydotool",
  xdotool: "xdotool",
};

export const TypingToolSetting = (props: TypingToolProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const [availableTools, setAvailableTools] = createSignal<string[] | null>(
    null,
  );

  createEffect(
    () => undefined,
    () => {
      if (osType !== "linux") return;
      commands
        .getAvailableTypingTools()
        .then(setAvailableTools)
        .catch(() => {
          setAvailableTools(["auto"]);
        });
    },
  );

  return (
    <Show when={osType === "linux" && getSetting("paste_method") === "direct"}>
      <SettingContainer
        title={t("settings.advanced.typingTool.title")}
        description={t("settings.advanced.typingTool.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
        tooltipPosition="bottom"
      >
        <Dropdown
          options={(availableTools() ?? ["auto"]).map((tool) =>
            tool === "auto"
              ? {
                  value: "auto",
                  label: t("settings.advanced.typingTool.options.auto"),
                }
              : { value: tool, label: allToolLabels[tool] ?? tool },
          )}
          selectedValue={(getSetting("typing_tool") || "auto") as TypingTool}
          onSelect={(value) =>
            updateSetting("typing_tool", value as TypingTool)
          }
          disabled={isUpdating("typing_tool")}
        />
      </SettingContainer>
    </Show>
  );
};
