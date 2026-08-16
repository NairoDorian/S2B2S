import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { PauseCircle, Trash2 } from "lucide-react";
import { useHubStore } from "@/stores/hubStore";
import { ModelsSettings } from "./ModelsSettings";
import { LlamaDownloadPanel, LlamaStatusCard } from "../brain/BrainSettings";
import { AudioCppModelManager } from "../speech/AudioCppModelManager";
import LlamaCppSettings from "../llama-cpp/LlamaCppSettings";
import { useLlamaState } from "@/hooks/useLlamaState";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/Tabs";
import { default as Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";

type HubCollection = "stt" | "brain" | "tts" | "runtime";

const collectionBadge: Record<
  HubCollection,
  { labelKey: string; color: string }
> = {
  stt: {
    labelKey: "settings.models.hub.collection.stt",
    color: "text-blue-400",
  },
  brain: {
    labelKey: "settings.models.hub.collection.brain",
    color: "text-logo-primary",
  },
  tts: {
    labelKey: "settings.models.hub.collection.tts",
    color: "text-emerald-400",
  },
  runtime: {
    labelKey: "settings.models.hub.collection.runtime",
    color: "text-amber-400",
  },
};

const BrainModelsTab: React.FC = () => {
  const llamaState = useLlamaState();
  return (
    <div className="space-y-4">
      <LlamaStatusCard />
      <LlamaDownloadPanel llamaState={llamaState} />
    </div>
  );
};

const TtsModelsTab: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="space-y-4">
      <p className="text-xs text-text/60">
        {t("settings.models.hub.ttsOpenSettings")}
      </p>
      <AudioCppModelManager />
    </div>
  );
};

const RuntimesTab: React.FC = () => <LlamaCppSettings />;

const HubActiveDownloadsBar: React.FC = () => {
  const { t } = useTranslation();
  const { downloads, cancelDownload, deleteModel } = useHubStore();

  const entries = Object.values(downloads);
  if (entries.length === 0) {
    return (
      <div className="text-xs text-text/60 py-2">
        {t("settings.models.hub.noActiveDownloads")}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="text-xs font-medium text-text/60">
        {t("settings.models.hub.activeDownloads")}
      </div>
      {entries.map((d) => {
        const c = collectionBadge[d.collection as HubCollection];
        const isTerminal =
          d.status === "completed" ||
          d.status === "failed" ||
          d.status === "cancelled";
        const percent = d.percent ?? 0;
        return (
          <div
            key={`${d.collection}:${d.id}:${d.file ?? "main"}`}
            className="flex items-center gap-3 text-sm"
          >
            <Badge className={c.color}>{t(c.labelKey)}</Badge>
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between">
                <span className="font-medium text-text/80 truncate">
                  {d.name}
                </span>
                <span className="text-text/60">
                  {d.speedMbps != null
                    ? `${d.speedMbps.toFixed(1)} MB/s`
                    : d.status}
                </span>
              </div>
              <div className="flex items-center gap-2 mt-0.5">
                <div className="flex-1 h-1.5 bg-mid-gray/20 rounded overflow-hidden">
                  <div
                    className="h-full bg-gradient-to-r from-logo-primary to-amber-500 rounded transition-all"
                    style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
                  />
                </div>
                <span className="text-[10px] text-text/50 w-10 text-right">
                  {Math.round(percent)}%
                </span>
              </div>
              {d.file && (
                <div className="text-[10px] text-text/40 truncate">
                  {d.file}
                </div>
              )}
            </div>
            {!isTerminal ? (
              <Button
                variant="danger-ghost"
                size="sm"
                className="h-6 px-1.5"
                title={t("settings.models.hub.cancel")}
                onClick={() => void cancelDownload(d.collection, d.id)}
              >
                <PauseCircle className="w-3.5 h-3.5" />
              </Button>
            ) : (
              <Button
                variant="danger-ghost"
                size="sm"
                className="h-6 px-1.5"
                title={t("settings.models.hub.remove")}
                onClick={() => void deleteModel(d.collection, d.id)}
              >
                <Trash2 className="w-3.5 h-3.5" />
              </Button>
            )}
          </div>
        );
      })}
    </div>
  );
};

export const ModelHubSettings: React.FC = () => {
  const { t } = useTranslation();
  useEffect(() => {
    void useHubStore.getState().initialize();
  }, []);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-4">
      <div className="mb-4">
        <h1 className="text-xl font-semibold mb-2">
          {t("settings.models.title")}
        </h1>
        <p className="text-sm text-text/60">
          {t("settings.models.description")}
        </p>
      </div>

      <HubActiveDownloadsBar />

      <Tabs defaultValue="speech" className="w-full">
        <TabsList className="mb-3">
          <TabsTrigger value="speech">
            {t("settings.models.hub.tabs.speech")}
          </TabsTrigger>
          <TabsTrigger value="brain">
            {t("settings.models.hub.tabs.brain")}
          </TabsTrigger>
          <TabsTrigger value="tts">
            {t("settings.models.hub.tabs.tts")}
          </TabsTrigger>
          <TabsTrigger value="runtimes">
            {t("settings.models.hub.tabs.runtimes")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="speech">
          <ModelsSettings hideTitle />
        </TabsContent>

        <TabsContent value="brain">
          <BrainModelsTab />
        </TabsContent>

        <TabsContent value="tts">
          <TtsModelsTab />
        </TabsContent>

        <TabsContent value="runtimes">
          <RuntimesTab />
        </TabsContent>
      </Tabs>
    </div>
  );
};
