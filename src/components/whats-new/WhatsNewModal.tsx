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
  const initialFocusRef: { current: HTMLElement | null } = { current: null };

  return (
    <Dialog
      open={props.open}
      title={t("whatsNew.title", { version: props.note.version })}
      closeLabel={t("common.close")}
      initialFocusRef={initialFocusRef}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) props.onDismiss();
      }}
    >
      <MarkdownContent markdown={props.note.markdown} />
    </Dialog>
  );
};
