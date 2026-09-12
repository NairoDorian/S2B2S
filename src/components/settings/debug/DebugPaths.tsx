import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";

interface DebugPathsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** The store file name, mirroring `settings::SETTINGS_STORE_PATH` in the backend. */
const SETTINGS_STORE_FILE = "settings_store.json";

/**
 * The three directories a support question usually needs, as absolute paths.
 *
 * All of them are derived from the app data directory the backend reports,
 * rather than written out here: the directory is named after the bundle
 * identifier (not the product name), it moves with portable mode, and it is
 * platform-dependent — so a literal would be wrong on at least one of those
 * axes on every machine. Fetching the one real path and appending the two
 * sub-paths keeps this correct by construction.
 *
 * The sub-path names are the ones the backend actually uses: `<app data>/models`
 * (`commands/mod.rs`, `managers/model.rs`) and `<app data>/settings_store.json`.
 */
export const DebugPaths: React.FC<DebugPathsProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [appDir, setAppDir] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const result = await commands.getAppDirPath();
        if (cancelled) return;
        if (result.status === "ok") {
          setAppDir(result.data);
        } else {
          setError(result.error);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, []);

  // Each row is a label and a path, in the order the keys are listed in the
  // locale files.
  const rows: [string, string | null][] = [
    [t("settings.debug.paths.appData"), appDir],
    [t("settings.debug.paths.models"), appDir ? `${appDir}/models` : null],
    [
      t("settings.debug.paths.settings"),
      appDir ? `${appDir}/${SETTINGS_STORE_FILE}` : null,
    ],
  ];

  return (
    <SettingContainer
      title={t("settings.debug.paths.title")}
      description={t("settings.debug.paths.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <div className="text-sm text-gray-600 space-y-2">
        {rows.map(([label, path]) => (
          <div key={label}>
            <span className="font-medium">{label}</span>{" "}
            {/* A filesystem path is literal data, never translated prose — an
                expression container says so without a lint suppression. */}
            <span className="font-mono text-xs select-text">
              {path ?? (error ? `— ${error}` : "…")}
            </span>
          </div>
        ))}
      </div>
    </SettingContainer>
  );
};
