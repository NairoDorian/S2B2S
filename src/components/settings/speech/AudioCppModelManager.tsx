import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  Check,
  Download,
  Flame,
  Globe,
  HardDrive,
  Loader2,
  RefreshCw,
  Search,
  Sparkles,
  Trash2,
  Volume2,
  X,
  Zap,
} from "lucide-react";
import {
  commands,
  events,
  type AudioCppDownloadProgress,
  type AudioCppModelFamily,
  type AudioCppPackageVariant,
} from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";

interface AudioCppModelManagerProps {
  onModelSelected?: () => void;
}

export const AudioCppModelManager: React.FC<AudioCppModelManagerProps> = ({
  onModelSelected,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const [families, setFamilies] = useState<AudioCppModelFamily[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterType, setFilterType] = useState<
    "all" | "installed" | "cloning" | "multilingual"
  >("all");
  const [selectedVariants, setSelectedVariants] = useState<
    Record<string, string>
  >({});
  const [downloads, setDownloads] = useState<
    Record<string, AudioCppDownloadProgress>
  >({});
  const [actionInProgress, setActionInProgress] = useState<string | null>(null);

  const fetchModels = async () => {
    try {
      setLoading(true);
      const res = await commands.audiocppListModels();
      if (res.status === "ok") {
        setFamilies(res.data);
        // Initialize default selected variant per family if not set
        setSelectedVariants((prev) => {
          const updated = { ...prev };
          for (const fam of res.data) {
            if (!updated[fam.family]) {
              const defaultPkg =
                fam.packages.find((p) => p.isDefault) || fam.packages[0];
              if (defaultPkg) {
                updated[fam.family] = defaultPkg.id;
              }
            }
          }
          return updated;
        });
      }
    } catch (err) {
      console.error("Failed to list audio.cpp models:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void fetchModels();

    // Listen for streaming download progress events
    const unlistenPromise = events.audioCppDownloadProgress.listen((event) => {
      const p = event.payload;
      setDownloads((prev) => {
        if (p.status === "completed" || p.status === "canceled") {
          const next = { ...prev };
          delete next[p.packageId];
          return next;
        }
        return { ...prev, [p.packageId]: p };
      });

      if (p.status === "completed") {
        void fetchModels();
      }
    });

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const handleDownload = async (packageId: string) => {
    try {
      setActionInProgress(packageId);
      const res = await commands.hubDownloadModel({
        collection: "tts",
        id: packageId,
      });
      if (res.status === "error") {
        console.error("Failed to start download:", res.error);
      }
    } catch (err) {
      console.error("Error starting download:", err);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleCancelDownload = async (packageId: string) => {
    try {
      await commands.hubCancelDownload("tts", packageId);
      setDownloads((prev) => {
        const next = { ...prev };
        delete next[packageId];
        return next;
      });
      await fetchModels();
    } catch (err) {
      console.error("Failed to cancel download:", err);
    }
  };

  const handleDelete = async (family: string, pkg: AudioCppPackageVariant) => {
    const confirmed = await ask(
      t("settings.speech.audiocppManager.deleteConfirm", {
        name: pkg.displayName || pkg.id,
      }),
      {
        title: t("settings.speech.audiocppManager.deleteTitle"),
        kind: "warning",
      },
    );
    if (!confirmed) return;

    try {
      setActionInProgress(pkg.id);
      const res = await commands.hubDeleteModel("tts", pkg.id);
      if (res.status === "ok") {
        await fetchModels();
      } else {
        console.error("Failed to delete package:", res.error);
      }
    } catch (err) {
      console.error("Error deleting package:", err);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleSelectActiveModel = async (
    family: string,
    packageId?: string,
  ) => {
    try {
      setActionInProgress(family);
      const res = await commands.audiocppSetActiveModel(
        family,
        packageId ?? null,
      );
      if (res.status === "ok") {
        if (settings?.tts) {
          await updateSetting("tts", {
            ...settings.tts,
            audiocpp: {
              ...settings.tts.audiocpp,
              model: family,
              quantization: packageId ?? "default",
            },
          });
        }
        await fetchModels();
        onModelSelected?.();
      } else {
        console.error("Failed to set active model:", res.error);
      }
    } catch (err) {
      console.error("Error setting active model:", err);
    } finally {
      setActionInProgress(null);
    }
  };

  const filteredFamilies = useMemo(() => {
    return families.filter((fam) => {
      const matchesSearch =
        searchQuery.trim() === "" ||
        fam.displayName.toLowerCase().includes(searchQuery.toLowerCase()) ||
        fam.family.toLowerCase().includes(searchQuery.toLowerCase()) ||
        fam.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
        fam.languages.some((l) =>
          l.toLowerCase().includes(searchQuery.toLowerCase()),
        );

      if (!matchesSearch) return false;

      if (filterType === "installed") {
        return fam.packages.some((p) => p.isDownloaded);
      }
      if (filterType === "cloning") {
        return (
          fam.tasks.includes("clone") ||
          fam.tasks.includes("vc") ||
          fam.capabilities.includes("clone")
        );
      }
      if (filterType === "multilingual") {
        return fam.languages.length > 5;
      }
      return true;
    });
  }, [families, searchQuery, filterType]);

  const activeModelId = settings?.tts?.audiocpp?.model || "supertonic";

  return (
    <div className="space-y-4">
      {/* Header Controls */}
      <div className="flex flex-col sm:flex-row gap-2 items-stretch sm:items-center justify-between">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("settings.speech.audiocppManager.searchPlaceholder")}
            className="pl-9 pr-8"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery("")}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* Filter Pills */}
        <div className="flex flex-wrap gap-1.5 items-center">
          {(
            [
              {
                id: "all",
                label: t("settings.speech.audiocppManager.filterAll"),
              },
              {
                id: "installed",
                label: t("settings.speech.audiocppManager.filterInstalled"),
              },
              {
                id: "cloning",
                label: t("settings.speech.audiocppManager.filterCloning"),
              },
              {
                id: "multilingual",
                label: t("settings.speech.audiocppManager.filterMultilingual"),
              },
            ] as const
          ).map((tab) => (
            <button
              key={tab.id}
              onClick={() => setFilterType(tab.id)}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-all ${
                filterType === tab.id
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "bg-muted/60 text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
            >
              {tab.label}
            </button>
          ))}

          <Button
            variant="ghost"
            size="sm"
            onClick={() => void fetchModels()}
            disabled={loading}
            className="h-8 px-2.5"
            title={t("settings.speech.audiocppManager.refresh")}
          >
            <RefreshCw
              className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`}
            />
          </Button>
        </div>
      </div>

      {/* Model Families Grid */}
      {loading && families.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-muted-foreground space-y-2">
          <Loader2 className="w-6 h-6 animate-spin text-primary" />
          <p className="text-sm">
            {t("settings.speech.audiocppManager.loadingModels")}
          </p>
        </div>
      ) : filteredFamilies.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <HardDrive className="w-8 h-8 mx-auto mb-2 opacity-40" />
          <p className="text-sm">
            {t("settings.speech.audiocppManager.noModelsFound")}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3.5">
          {filteredFamilies.map((fam) => {
            const isFamilyActive = fam.family === activeModelId;
            const currentPkgId =
              selectedVariants[fam.family] ||
              fam.packages.find((p) => p.isDefault)?.id ||
              fam.packages[0]?.id;
            const currentPkg =
              fam.packages.find((p) => p.id === currentPkgId) ||
              fam.packages[0];
            const isInstalled = Boolean(currentPkg?.isDownloaded);
            const activeDownload = currentPkg ? downloads[currentPkg.id] : null;
            const isAnyVariantInstalled = fam.packages.some(
              (p) => p.isDownloaded,
            );

            return (
              <div
                key={fam.family}
                className={`relative rounded-xl p-4 transition-all duration-200 border flex flex-col justify-between ${
                  isFamilyActive
                    ? "bg-primary/5 border-primary shadow-sm"
                    : "bg-card hover:bg-muted/30 border-border/80"
                }`}
              >
                <div>
                  {/* Card Header */}
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <div>
                      <div className="flex items-center gap-2 flex-wrap">
                        <h4 className="font-semibold text-sm text-foreground">
                          {fam.displayName}
                        </h4>
                        {isFamilyActive && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-primary text-primary-foreground">
                            <Check className="w-3 h-3" />
                            {t("settings.speech.audiocppManager.active")}
                          </span>
                        )}
                        {fam.tasks.includes("clone") && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-amber-500/10 text-amber-500 border border-amber-500/20">
                            <Zap className="w-2.5 h-2.5" />
                            {t("settings.speech.audiocppManager.voiceClone")}
                          </span>
                        )}
                        {fam.tasks.includes("design") && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-purple-500/10 text-purple-500 border border-purple-500/20">
                            <Sparkles className="w-2.5 h-2.5" />
                            {t("settings.speech.audiocppManager.voiceDesign")}
                          </span>
                        )}
                      </div>
                      <p className="text-xs text-muted-foreground mt-1 line-clamp-2 leading-relaxed">
                        {fam.description}
                      </p>
                    </div>
                  </div>

                  {/* Languages & Capabilities */}
                  <div className="flex flex-wrap items-center gap-2 my-2.5 text-[11px] text-muted-foreground">
                    {fam.languages.length > 0 && (
                      <span className="inline-flex items-center gap-1 bg-muted/70 px-2 py-0.5 rounded">
                        <Globe className="w-3 h-3 text-muted-foreground" />
                        {fam.languages.length > 5
                          ? t(
                              "settings.speech.audiocppManager.languagesCount",
                              {
                                count: fam.languages.length,
                              },
                            )
                          : fam.languages.join(", ").toUpperCase()}
                      </span>
                    )}
                    {fam.modes.includes("streaming") && (
                      <span className="inline-flex items-center gap-1 bg-emerald-500/10 text-emerald-500 px-2 py-0.5 rounded border border-emerald-500/20">
                        <Flame className="w-3 h-3" />
                        {t("settings.speech.audiocppManager.streaming")}
                      </span>
                    )}
                  </div>

                  {/* Quantization Variant Picker */}
                  {fam.packages.length > 0 && (
                    <div className="mt-3 pt-3 border-t border-border/50">
                      <label className="text-[11px] font-medium text-muted-foreground block mb-1.5">
                        {t("settings.speech.audiocppManager.quantizationLabel")}
                        :
                      </label>
                      <div className="grid grid-cols-1 gap-1.5">
                        {fam.packages.map((pkg) => {
                          const isSelected = pkg.id === currentPkgId;
                          const isPkgDownloaded = pkg.isDownloaded;
                          const pkgDownload = downloads[pkg.id];
                          const isPkgActive =
                            isFamilyActive &&
                            (settings?.tts?.audiocpp?.quantization === pkg.id ||
                              (!settings?.tts?.audiocpp?.quantization &&
                                pkg.isDefault));

                          return (
                            <div
                              key={pkg.id}
                              onClick={() =>
                                setSelectedVariants((prev) => ({
                                  ...prev,
                                  [fam.family]: pkg.id,
                                }))
                              }
                              className={`flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs transition-all text-left border cursor-pointer ${
                                isSelected
                                  ? "bg-accent/80 text-accent-foreground border-primary/60 font-medium shadow-xs"
                                  : "bg-muted/30 hover:bg-muted/60 text-muted-foreground border-transparent"
                              }`}
                            >
                              <div className="flex items-center gap-2 truncate">
                                <span className="font-mono uppercase text-[10px] bg-background/80 px-1.5 py-0.5 rounded border border-border/60">
                                  {pkg.precision}
                                </span>
                                <span className="truncate">
                                  {pkg.displayName || pkg.id}
                                </span>
                                {isPkgActive && (
                                  <span className="text-[9px] px-1.5 py-0.5 rounded bg-primary/20 text-primary font-semibold flex items-center gap-1">
                                    <Check className="w-2.5 h-2.5" />
                                    {t(
                                      "settings.speech.audiocppManager.active",
                                    )}
                                  </span>
                                )}
                              </div>
                              <div className="flex items-center gap-2 flex-shrink-0 text-[11px]">
                                <span className="text-muted-foreground">
                                  {pkg.sizeMb} MB
                                </span>
                                {isPkgDownloaded ? (
                                  <Check className="w-3.5 h-3.5 text-emerald-500 stroke-[2.5]" />
                                ) : pkgDownload ? (
                                  <Loader2 className="w-3.5 h-3.5 animate-spin text-primary" />
                                ) : (
                                  <button
                                    type="button"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      setSelectedVariants((prev) => ({
                                        ...prev,
                                        [fam.family]: pkg.id,
                                      }));
                                      void handleDownload(pkg.id);
                                    }}
                                    title={t(
                                      "settings.speech.audiocppManager.downloadPackage",
                                      { size: pkg.sizeMb },
                                    )}
                                    className="p-1 rounded hover:bg-primary/20 text-muted-foreground hover:text-primary transition-colors"
                                  >
                                    <Download className="w-3.5 h-3.5" />
                                  </button>
                                )}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  )}
                </div>

                {/* Card Action Footer */}
                <div className="mt-4 pt-3 border-t border-border/60 flex flex-col gap-2">
                  {/* Download Progress Bar if actively downloading */}
                  {activeDownload && (
                    <div className="space-y-1.5 bg-muted/40 p-2.5 rounded-lg border border-border/60">
                      <div className="flex items-center justify-between text-xs">
                        <span className="font-medium text-foreground flex items-center gap-1.5">
                          <Loader2 className="w-3 h-3 animate-spin text-primary" />
                          {t("settings.speech.audiocppManager.downloading")} (
                          {(activeDownload.percent ?? 0).toFixed(1)}%)
                        </span>
                        <span className="text-muted-foreground text-[11px]">
                          {(activeDownload.speedMbps ?? 0) > 0 &&
                            `${(activeDownload.speedMbps ?? 0).toFixed(1)} MB/s`}
                        </span>
                      </div>
                      <div className="w-full bg-muted rounded-full h-1.5 overflow-hidden">
                        <div
                          className="bg-primary h-full transition-all duration-300 rounded-full"
                          style={{
                            width: `${Math.min(100, Math.max(0, activeDownload.percent ?? 0))}%`,
                          }}
                        />
                      </div>
                      <div className="flex justify-end">
                        <Button
                          variant="danger-ghost"
                          size="sm"
                          onClick={() =>
                            void handleCancelDownload(currentPkg.id)
                          }
                          className="h-6 px-2 text-[11px]"
                        >
                          {t("settings.speech.audiocppManager.cancel")}
                        </Button>
                      </div>
                    </div>
                  )}

                  {/* Normal Action Buttons */}
                  {!activeDownload && currentPkg && (
                    <div className="flex items-center justify-between gap-2">
                      {isInstalled ? (
                        <>
                          <Button
                            variant={
                              isFamilyActive &&
                              settings?.tts?.audiocpp?.quantization ===
                                currentPkg.id
                                ? "secondary"
                                : "primary"
                            }
                            size="sm"
                            onClick={() =>
                              void handleSelectActiveModel(
                                fam.family,
                                currentPkg.id,
                              )
                            }
                            disabled={
                              (isFamilyActive &&
                                settings?.tts?.audiocpp?.quantization ===
                                  currentPkg.id) ||
                              actionInProgress === fam.family
                            }
                            className="flex-1 text-xs h-8"
                          >
                            {isFamilyActive &&
                            settings?.tts?.audiocpp?.quantization ===
                              currentPkg.id ? (
                              <>
                                <Check className="w-3.5 h-3.5 mr-1.5 text-primary" />
                                {t(
                                  "settings.speech.audiocppManager.inUseActive",
                                )}
                              </>
                            ) : (
                              <>
                                <Volume2 className="w-3.5 h-3.5 mr-1.5" />
                                {t("settings.speech.audiocppManager.useModel")}
                              </>
                            )}
                          </Button>
                          <Button
                            variant="danger-ghost"
                            size="sm"
                            onClick={() =>
                              void handleDelete(fam.family, currentPkg)
                            }
                            disabled={actionInProgress === currentPkg.id}
                            className="h-8 px-2.5 text-muted-foreground hover:text-destructive"
                            title={t("settings.speech.audiocppManager.delete")}
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => void handleDownload(currentPkg.id)}
                          disabled={actionInProgress === currentPkg.id}
                          className="w-full text-xs h-8 bg-muted/40 hover:bg-primary hover:text-primary-foreground transition-all"
                        >
                          {actionInProgress === currentPkg.id ? (
                            <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                          ) : (
                            <Download className="w-3.5 h-3.5 mr-1.5" />
                          )}
                          {t(
                            "settings.speech.audiocppManager.downloadPackage",
                            {
                              size: currentPkg.sizeMb,
                            },
                          )}
                        </Button>
                      )}
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
