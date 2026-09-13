import { createSignal, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { sessionToast as toast } from "@/lib/sessionToast";
import { Button } from "../../ui/Button";
import { SettingContainer } from "../../ui/SettingContainer";
import { WhatsNewModal } from "../../whats-new/WhatsNewModal";
import { findLatestReleaseNote } from "../../whats-new/releaseNotes";
import type { ReleaseNote } from "../../whats-new/releaseNotes";

interface WhatsNewPreviewProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const WhatsNewPreview = ({
  descriptionMode = "tooltip",
  grouped = false,
}: WhatsNewPreviewProps) => {
  const { t } = useTranslation();
  const [note, setNote] = createSignal<ReleaseNote | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);

  const preview = () => {
    setIsLoading(true);

    try {
      const releaseNote = findLatestReleaseNote();

      if (!releaseNote) {
        toast.info(t("settings.debug.whatsNewPreview.noNotes"));
        return;
      }

      setNote(releaseNote);
    } catch (error) {
      console.error("Failed to preview release notes:", error);
      toast.error(t("settings.debug.whatsNewPreview.error"));
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <>
      <SettingContainer
        title={t("settings.debug.whatsNewPreview.title")}
        description={t("settings.debug.whatsNewPreview.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Button
          variant="secondary"
          size="md"
          onClick={preview}
          disabled={isLoading()}
        >
          {t("settings.debug.whatsNewPreview.button")}
        </Button>
      </SettingContainer>

      <Show when={note()}>
        {(releaseNote) => (
          <WhatsNewModal
            note={releaseNote()}
            open={true}
            onDismiss={() => setNote(null)}
          />
        )}
      </Show>
    </>
  );
};
