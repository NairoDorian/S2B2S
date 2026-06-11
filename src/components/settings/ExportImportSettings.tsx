import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { save, open } from "@tauri-apps/plugin-dialog";
import { SettingContainer } from "../ui/SettingContainer";
import { Button } from "../ui/Button";

interface ExportImportSettingsProps {
  grouped?: boolean;
}

export const ExportImportSettings: React.FC<ExportImportSettingsProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);

  const handleExport = async () => {
    try {
      setExporting(true);
      const filePath = await save({
        filters: [
          {
            name: "JSON",
            extensions: ["json"],
          },
        ],
        defaultPath: "s2b2s-settings.json",
      });

      if (!filePath) {
        setExporting(false);
        return;
      }

      const result = await commands.exportSettings(filePath);
      if (result.status === "ok") {
        toast.success(
          t("settings.about.exportSuccess", {
            defaultValue: "Settings exported successfully!",
          }),
        );
      } else {
        toast.error(
          t("settings.about.exportError", {
            defaultValue: `Export failed: ${result.error}`,
          }),
        );
      }
    } catch (err) {
      console.error(err);
      toast.error(String(err));
    } finally {
      setExporting(false);
    }
  };

  const handleImport = async () => {
    try {
      setImporting(true);
      const filePath = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "JSON",
            extensions: ["json"],
          },
        ],
      });

      if (!filePath) {
        setImporting(false);
        return;
      }

      const pathStr = Array.isArray(filePath) ? filePath[0] : filePath;
      if (!pathStr) {
        setImporting(false);
        return;
      }

      const result = await commands.importSettings(pathStr);
      if (result.status === "ok") {
        toast.success(
          t("settings.about.importSuccess", {
            defaultValue: "Settings imported successfully! Reloading...",
          }),
        );
        setTimeout(() => {
          window.location.reload();
        }, 1000);
      } else {
        toast.error(
          t("settings.about.importError", {
            defaultValue: `Import failed: ${result.error}`,
          }),
        );
      }
    } catch (err) {
      console.error(err);
      toast.error(String(err));
    } finally {
      setImporting(false);
    }
  };

  return (
    <>
      <SettingContainer
        title={t("settings.about.exportSettings.title", {
          defaultValue: "Export Settings",
        })}
        description={t("settings.about.exportSettings.description", {
          defaultValue: "Save your settings configuration to a file.",
        })}
        grouped={grouped}
      >
        <Button
          variant="secondary"
          size="md"
          onClick={handleExport}
          disabled={exporting || importing}
        >
          {exporting
            ? t("settings.about.exportSettings.exporting", {
                defaultValue: "Exporting...",
              })
            : t("settings.about.exportSettings.button", {
                defaultValue: "Export",
              })}
        </Button>
      </SettingContainer>

      <SettingContainer
        title={t("settings.about.importSettings.title", {
          defaultValue: "Import Settings",
        })}
        description={t("settings.about.importSettings.description", {
          defaultValue: "Load your settings configuration from a backup file.",
        })}
        grouped={grouped}
      >
        <Button
          variant="secondary"
          size="md"
          onClick={handleImport}
          disabled={exporting || importing}
        >
          {importing
            ? t("settings.about.importSettings.importing", {
                defaultValue: "Importing...",
              })
            : t("settings.about.importSettings.button", {
                defaultValue: "Import",
              })}
        </Button>
      </SettingContainer>
    </>
  );
};
