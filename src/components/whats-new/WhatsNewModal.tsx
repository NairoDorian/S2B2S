import { useTranslation } from "@/i18n/useTranslation";
import { Dialog } from "../ui";
import { MarkdownContent } from "./MarkdownContent";
import type { ReleaseNote } from "./releaseNotes";

interface WhatsNewModalProps {
  note: ReleaseNote;
  open: boolean;
  onDismiss: () => void;
}

export const WhatsNewModal = (props: WhatsNewModalProps) => {
  const { t } = useTranslation();

  return (
    <Dialog
      open={props.open}
      title={t("whatsNew.title", { version: props.note.version })}
      closeLabel={t("common.close")}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) props.onDismiss();
      }}
    >
      <MarkdownContent markdown={props.note.markdown} />
    </Dialog>
  );
};
