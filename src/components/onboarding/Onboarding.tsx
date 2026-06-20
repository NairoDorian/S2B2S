import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Loader2 } from "lucide-react";
import type { ModelInfo } from "@/bindings";
import type { ModelCardStatus } from "./ModelCard";
import ModelCard from "./ModelCard";
import S2B2STextLogo from "../icons/S2B2STextLogo";
import { useModelStore } from "../../stores/modelStore";
import { commands } from "@/bindings";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../ui/Button";

interface OnboardingProps {
  onModelSelected: () => void;
}

const Onboarding: React.FC<OnboardingProps> = ({ onModelSelected }) => {
  const { t } = useTranslation();
  const {
    models,
    downloadModel,
    selectModel,
    downloadingModels,
    verifyingModels,
    extractingModels,
    downloadProgress,
    downloadStats,
  } = useModelStore();
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);

  const [runtimeStatus, setRuntimeStatus] = useState<
    "checking" | "not_installed" | "installing" | "installed" | "failed"
  >("checking");
  const [installProgressMessage, setInstallProgressMessage] = useState<string>("");
  const [errorMessage, setErrorMessage] = useState<string>("");

  const isDownloading = selectedModelId !== null;

  // 1. Check if the speech runtime is installed on mount
  useEffect(() => {
    commands
      .checkSpeechRuntimeInstalled()
      .then((installed) => {
        if (installed) {
          setRuntimeStatus("installed");
        } else {
          setRuntimeStatus("not_installed");
        }
      })
      .catch((err) => {
        console.error("Failed to check speech runtime status:", err);
        setRuntimeStatus("not_installed");
      });
  }, []);

  // 2. Listen for runtime installation events
  useEffect(() => {
    if (runtimeStatus !== "installing") return;

    let unlistenProgress: (() => void) | null = null;
    let unlistenSuccess: (() => void) | null = null;
    let unlistenFailed: (() => void) | null = null;

    const setupListeners = async () => {
      unlistenProgress = await listen<{ message: string }>(
        "runtime-install-progress",
        (event) => {
          setInstallProgressMessage(event.payload.message);
        }
      );

      unlistenSuccess = await listen("runtime-install-success", () => {
        toast.success(t("onboarding.runtime.success", "Speech runtime installed successfully!"));
        setRuntimeStatus("installed");
      });

      unlistenFailed = await listen<string>("runtime-install-failed", (event) => {
        const err = event.payload || "Unknown error occurred";
        setErrorMessage(err);
        setRuntimeStatus("failed");
        toast.error(t("onboarding.runtime.failedTitle", "Installation Failed"), {
          description: err,
        });
      });
    };

    setupListeners();

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenSuccess) unlistenSuccess();
      if (unlistenFailed) unlistenFailed();
    };
  }, [runtimeStatus, t]);

  const handleInstallRuntime = async () => {
    setRuntimeStatus("installing");
    setInstallProgressMessage(t("onboarding.runtime.starting", "Initializing installation..."));
    try {
      await commands.installSpeechRuntime();
    } catch (err: any) {
      setErrorMessage(err?.toString() || "Failed to start install process");
      setRuntimeStatus("failed");
      toast.error(t("onboarding.runtime.startFailed", "Failed to start installation"));
    }
  };

  // 3. Watch for the selected model to finish downloading + verifying + extracting
  useEffect(() => {
    if (!selectedModelId) return;

    const model = models.find((m) => m.id === selectedModelId);
    const stillDownloading = selectedModelId in downloadingModels;
    const stillVerifying = selectedModelId in verifyingModels;
    const stillExtracting = selectedModelId in extractingModels;

    if (
      model?.is_downloaded &&
      !stillDownloading &&
      !stillVerifying &&
      !stillExtracting
    ) {
      // Model is ready — select it and transition
      selectModel(selectedModelId).then((success) => {
        if (success) {
          onModelSelected();
        } else {
          toast.error(t("onboarding.errors.selectModel"));
          setSelectedModelId(null);
        }
      });
    }
  }, [
    selectedModelId,
    models,
    downloadingModels,
    verifyingModels,
    extractingModels,
    selectModel,
    onModelSelected,
    t,
  ]);

  const handleDownloadModel = async (modelId: string) => {
    setSelectedModelId(modelId);

    const success = await downloadModel(modelId);
    if (!success) {
      setSelectedModelId(null);
    }
  };

  const getModelStatus = (modelId: string): ModelCardStatus => {
    if (modelId in extractingModels) return "extracting";
    if (modelId in verifyingModels) return "verifying";
    if (modelId in downloadingModels) return "downloading";
    return "downloadable";
  };

  const getModelDownloadProgress = (modelId: string): number | undefined => {
    return downloadProgress[modelId]?.percentage;
  };

  const getModelDownloadSpeed = (modelId: string): number | undefined => {
    return downloadStats[modelId]?.speed;
  };

  // RENDER: Checking Speech Runtime Status
  if (runtimeStatus === "checking") {
    return (
      <div className="h-screen w-screen flex flex-col items-center justify-center p-6 gap-4">
        <S2B2STextLogo width={200} />
        <div className="flex flex-col items-center gap-2 mt-8">
          <Loader2 className="animate-spin h-8 w-8 text-primary" />
          <p className="text-text/70 font-medium mt-2">
            {t("onboarding.runtime.checking", "Checking speech environment...")}
          </p>
        </div>
      </div>
    );
  }

  // RENDER: Speech Runtime Not Installed
  if (runtimeStatus === "not_installed") {
    return (
      <div className="h-screen w-screen flex flex-col p-6 justify-center items-center gap-6 inset-0 overflow-y-auto">
        <div className="flex flex-col items-center gap-2 shrink-0 max-w-md text-center">
          <S2B2STextLogo width={200} />
          <h2 className="text-2xl font-bold mt-6 text-text">
            {t("onboarding.runtime.setupTitle", "Setup Speech Runtime")}
          </h2>
          <p className="text-text/70 mt-2 font-medium leading-relaxed">
            {t(
              "onboarding.runtime.setupSubtitle",
              "S2B2S runs local, offline speech services. To support local voice activity detection (VAD) and voice synthesis (TTS), we need to configure a local runtime environment.",
            )}
          </p>
        </div>

        <div className="bg-mid-gray/5 border border-mid-gray/20 rounded-xl p-6 max-w-lg w-full flex flex-col gap-4 text-sm text-text/80">
          <div className="flex items-start gap-3">
            <span className="text-logo-primary font-bold">1.</span>
            <p>{t("onboarding.runtime.step1", "Downloads a portable uv package manager matching your system.")}</p>
          </div>
          <div className="flex items-start gap-3">
            <span className="text-logo-primary font-bold">2.</span>
            <p>{t("onboarding.runtime.step2", "Configures a standalone relocatable Python 3.12 environment.")}</p>
          </div>
          <div className="flex items-start gap-3">
            <span className="text-logo-primary font-bold">3.</span>
            <p>{t("onboarding.runtime.step3", "Installs speech models and dependency packages locally.")}</p>
          </div>
          <p className="text-xs text-text/50 border-t border-mid-gray/10 pt-3 mt-1">
            {t(
              "onboarding.runtime.portableNote",
              "Note: All downloads are stored inside the application data directory. No changes will be made to your system-wide Python installation.",
            )}
          </p>
        </div>

        <Button
          onClick={handleInstallRuntime}
          variant="primary"
          className="bg-logo-primary hover:bg-logo-primary-hover text-primary-foreground font-semibold px-8 py-3 rounded-lg shadow-lg hover:shadow-xl transition-all cursor-pointer mt-4"
        >
          {t("onboarding.runtime.installBtn", "Install Speech Runtime (~150MB)")}
        </Button>
      </div>
    );
  }

  // RENDER: Speech Runtime Installing
  if (runtimeStatus === "installing") {
    return (
      <div className="h-screen w-screen flex flex-col p-6 justify-center items-center gap-6 inset-0">
        <div className="flex flex-col items-center gap-2 shrink-0 max-w-md text-center">
          <S2B2STextLogo width={200} />
          <h2 className="text-2xl font-bold mt-6 text-text animate-pulse">
            {t("onboarding.runtime.installingTitle", "Installing Speech Runtime...")}
          </h2>
          <p className="text-text/70 mt-2 font-medium">
            {t(
              "onboarding.runtime.installingSubtitle",
              "Downloading uv and portable python environment. This may take a minute or two.",
            )}
          </p>
        </div>

        <div className="bg-mid-gray/5 border border-mid-gray/20 rounded-xl p-6 max-w-lg w-full flex flex-col items-center gap-4">
          <div className="flex items-center gap-3">
            <Loader2 className="animate-spin h-6 w-6 text-primary" />
            <span className="font-semibold text-text">{t("onboarding.runtime.processing", "Processing...")}</span>
          </div>

          <div className="w-full bg-mid-gray/20 h-2 rounded-full overflow-hidden">
            <div className="bg-logo-primary h-full w-2/3 animate-pulse rounded-full"></div>
          </div>

          <p className="text-sm text-text/80 bg-background/50 border border-mid-gray/10 rounded-lg p-3 w-full font-mono break-all text-center min-h-[44px]">
            {installProgressMessage || t("onboarding.runtime.waiting", "Setting up local environment...")}
          </p>
        </div>
      </div>
    );
  }

  // RENDER: Speech Runtime Installation Failed
  if (runtimeStatus === "failed") {
    return (
      <div className="h-screen w-screen flex flex-col p-6 justify-center items-center gap-6 inset-0 overflow-y-auto">
        <div className="flex flex-col items-center gap-2 shrink-0 max-w-md text-center">
          <S2B2STextLogo width={200} />
          <h2 className="text-2xl font-bold mt-6 text-red-500">
            {t("onboarding.runtime.failedTitle", "Installation Failed")}
          </h2>
          <p className="text-text/70 mt-2 font-medium">
            {t("onboarding.runtime.failedSubtitle", "An error occurred while installing the local speech runtime.")}
          </p>
        </div>

        <div className="bg-red-500/5 border border-red-500/20 rounded-xl p-6 max-w-lg w-full flex flex-col gap-3">
          <h3 className="font-bold text-red-400 text-sm">{t("onboarding.runtime.errorLog", "Error Details:")}</h3>
          <p className="text-xs text-text/80 bg-background/50 border border-red-500/10 rounded-lg p-3 w-full font-mono break-words max-h-40 overflow-y-auto">
            {errorMessage || t("onboarding.runtime.unknownError", "An unknown error occurred.")}
          </p>
        </div>

        <div className="flex gap-4">
          <Button
            onClick={handleInstallRuntime}
            variant="primary"
            className="bg-logo-primary hover:bg-logo-primary-hover text-primary-foreground font-semibold"
          >
            {t("onboarding.runtime.retryBtn", "Retry Installation")}
          </Button>
        </div>
      </div>
    );
  }

  // RENDER: Speech Runtime Installed (Proceed to Model Selection)
  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-4 inset-0">
      <div className="flex flex-col items-center gap-2 shrink-0">
        <S2B2STextLogo width={200} />
        <p className="text-text/70 max-w-md font-medium mx-auto">
          {t("onboarding.subtitle")}
        </p>
      </div>

      <div className="max-w-[600px] w-full mx-auto text-center flex-1 flex flex-col min-h-0">
        <div className="flex flex-col gap-4 pb-6 overflow-y-auto">
          {models
            .filter((m: ModelInfo) => !m.is_downloaded)
            .filter((model: ModelInfo) => model.is_recommended)
            .map((model: ModelInfo) => (
              <ModelCard
                key={model.id}
                model={model}
                variant="featured"
                status={getModelStatus(model.id)}
                disabled={isDownloading}
                onSelect={handleDownloadModel}
                onDownload={handleDownloadModel}
                downloadProgress={getModelDownloadProgress(model.id)}
                downloadSpeed={getModelDownloadSpeed(model.id)}
              />
            ))}

          {models
            .filter((m: ModelInfo) => !m.is_downloaded)
            .filter((model: ModelInfo) => !model.is_recommended)
            .sort(
              (a: ModelInfo, b: ModelInfo) =>
                Number(a.size_mb) - Number(b.size_mb),
            )
            .map((model: ModelInfo) => (
              <ModelCard
                key={model.id}
                model={model}
                status={getModelStatus(model.id)}
                disabled={isDownloading}
                onSelect={handleDownloadModel}
                onDownload={handleDownloadModel}
                downloadProgress={getModelDownloadProgress(model.id)}
                downloadSpeed={getModelDownloadSpeed(model.id)}
              />
            ))}
        </div>
      </div>
    </div>
  );
};

export default Onboarding;
