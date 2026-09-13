import { useTranslation } from "@/i18n/useTranslation";
import { Button } from "./Button";
import type { JSX } from "@solidjs/web";

interface PathDisplayProps {
  path: string;
  onOpen: () => void;
  disabled?: boolean;
}

export const PathDisplay = (props: PathDisplayProps): JSX.Element => {
  const { path, onOpen, disabled = false } = props;
  const { t } = useTranslation();

  return (
    <div class="flex items-center gap-2">
      <div class="flex-1 min-w-0 px-2 py-2 bg-mid-gray/10 border border-mid-gray/80 rounded-lg text-xs font-mono break-all select-text cursor-text">
        {path}
      </div>
      <Button
        onClick={onOpen}
        variant="secondary"
        size="sm"
        disabled={disabled}
        class="px-3 py-2"
      >
        {t("common.open")}
      </Button>
    </div>
  );
};
