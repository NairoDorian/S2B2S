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
  const { note, open, onDismiss } = props;
  const { t } = useTranslation();
  const initialFocusRef: { current: HTMLElement | null } = { current: null };

  return (
    <Dialog
      open={open}
      title={t("whatsNew.title", { version: note.version })}
      closeLabel={t("common.close")}
      initialFocusRef={initialFocusRef}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onDismiss();
      }}
    >
      <MarkdownContent markdown={note.markdown} />
    </Dialog>
  );
};
