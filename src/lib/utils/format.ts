import { useTranslation } from "@/i18n/useTranslation";

const { t } = useTranslation();

export const formatModelSize = (sizeMb: number | null | undefined): string => {
  if (!sizeMb || !Number.isFinite(sizeMb) || sizeMb <= 0) {
    return t("common.unknownSize");
  }

  if (sizeMb >= 1024) {
    const sizeGb = sizeMb / 1024;
    const formatter = new Intl.NumberFormat(undefined, {
      minimumFractionDigits: sizeGb >= 10 ? 0 : 1,
      maximumFractionDigits: sizeGb >= 10 ? 0 : 1,
    });
    return `${formatter.format(sizeGb)} GB`;
  }

  const formatter = new Intl.NumberFormat(undefined, {
    minimumFractionDigits: sizeMb >= 100 ? 0 : 1,
    maximumFractionDigits: sizeMb >= 100 ? 0 : 1,
  });

  return `${formatter.format(sizeMb)} MB`;
};
