import { untrack } from "solid-js";
import type { Accessor } from "solid-js";
import { useSettingsStore } from "../stores/settingsStore";
import type {
  AppSettings as Settings,
  AudioDevice,
  ModelBackendSetting,
  MultiSttExtraModel_Serialize as MultiSttExtraModel,
} from "@/bindings";

/**
 * The settings surface every page reads.
 *
 * Reactive values are **accessors**, not snapshots. A Solid component body runs
 * once, so `const { settings } = useSettings(); settings?.debug_mode` would read
 * the store at mount — when settings are still `null` — and never again. Call
 * the accessor where the value is used (`settings()?.debug_mode` inside JSX) and
 * the expression subscribes to just that property.
 *
 * Actions (`updateSetting`, `refreshSettings`, …) are stable functions and are
 * safe to destructure; `getSetting` / `isUpdating` are plain functions whose
 * *calls* are tracked, so calling them inside JSX is reactive.
 */
export interface UseSettingsResult {
  settings: Accessor<Settings | null>;
  isLoading: Accessor<boolean>;
  isUpdating: (key: string) => boolean;
  audioDevices: Accessor<AudioDevice[]>;
  outputDevices: Accessor<AudioDevice[]>;
  audioFeedbackEnabled: Accessor<boolean>;
  postProcessModelOptions: Accessor<Record<string, string[]>>;
  updateChecksLocked: Accessor<boolean | null>;

  updateSetting: <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => Promise<void>;
  resetSetting: (key: keyof Settings) => Promise<void>;
  /** Change one extra Multi-STT slot ("Model N", 2 = first extra). */
  updateMultiSttExtraModel: (
    slot: number,
    patch: Partial<MultiSttExtraModel>,
  ) => Promise<void>;
  /** How many extra Multi-STT slots there are. */
  setMultiSttExtraModelCount: (count: number) => Promise<void>;
  refreshSettings: () => Promise<void>;
  refreshAudioDevices: () => Promise<void>;
  refreshOutputDevices: () => Promise<void>;
  updateBinding: (id: string, binding: string) => Promise<void>;
  resetBinding: (id: string) => Promise<void>;
  getSetting: <K extends keyof Settings>(key: K) => Settings[K] | undefined;
  setPostProcessProvider: (providerId: string) => Promise<void>;
  /** Pin one model to a compute backend (or clear the pin with `auto`). */
  setModelBackend: (
    modelId: string,
    backend: ModelBackendSetting,
  ) => Promise<void>;
  updatePostProcessBaseUrl: (
    providerId: string,
    baseUrl: string,
  ) => Promise<void>;
  updatePostProcessApiKey: (
    providerId: string,
    apiKey: string,
  ) => Promise<void>;
  updatePostProcessModel: (providerId: string, model: string) => Promise<void>;
  fetchPostProcessModels: (providerId: string) => Promise<string[]>;
}

/**
 * Module-level guard so the ~100 components that call this hook do not each
 * kick off their own `initialize()`. The store starts with `isLoading: true`,
 * so without this the settings IPC would be issued once per mounted component.
 */
let initStarted = false;

export const useSettings = (): UseSettingsResult => {
  const store = useSettingsStore();

  if (!initStarted) {
    initStarted = true;
    void store.initialize();
  }

  // The action entries below snapshot the store's function references once.
  // Reading `store.updateSetting` etc. directly in this body would trip
  // `STRICT_READ_UNTRACKED` (14 untracked store reads per component — this
  // hook is called by ~100 of them); the refs are stable for the app's
  // lifetime, so the one-time read is intentional and wrapped in `untrack`
  // to say so. The state entries stay accessors so their reads land in
  // whatever tracking scope calls them.
  const actions = untrack(() => ({
    updateSetting: store.updateSetting,
    resetSetting: store.resetSetting,
    updateMultiSttExtraModel: store.updateMultiSttExtraModel,
    setMultiSttExtraModelCount: store.setMultiSttExtraModelCount,
    refreshSettings: store.refreshSettings,
    refreshAudioDevices: store.refreshAudioDevices,
    refreshOutputDevices: store.refreshOutputDevices,
    updateBinding: store.updateBinding,
    resetBinding: store.resetBinding,
    getSetting: store.getSetting,
    isUpdating: store.isUpdatingKey,
    setPostProcessProvider: store.setPostProcessProvider,
    setModelBackend: store.setModelBackend,
    updatePostProcessBaseUrl: store.updatePostProcessBaseUrl,
    updatePostProcessApiKey: store.updatePostProcessApiKey,
    updatePostProcessModel: store.updatePostProcessModel,
    fetchPostProcessModels: store.fetchPostProcessModels,
  }));

  return {
    settings: () => store.settings,
    isLoading: () => store.isLoading,
    isUpdating: actions.isUpdating,
    audioDevices: () => store.audioDevices,
    outputDevices: () => store.outputDevices,
    audioFeedbackEnabled: () => store.settings?.audio_feedback ?? false,
    postProcessModelOptions: () => store.postProcessModelOptions,
    updateChecksLocked: () => store.updateChecksLocked,

    updateSetting: actions.updateSetting,
    resetSetting: actions.resetSetting,
    updateMultiSttExtraModel: actions.updateMultiSttExtraModel,
    setMultiSttExtraModelCount: actions.setMultiSttExtraModelCount,
    refreshSettings: actions.refreshSettings,
    refreshAudioDevices: actions.refreshAudioDevices,
    refreshOutputDevices: actions.refreshOutputDevices,
    updateBinding: actions.updateBinding,
    resetBinding: actions.resetBinding,
    getSetting: actions.getSetting,
    setPostProcessProvider: actions.setPostProcessProvider,
    setModelBackend: actions.setModelBackend,
    updatePostProcessBaseUrl: actions.updatePostProcessBaseUrl,
    updatePostProcessApiKey: actions.updatePostProcessApiKey,
    updatePostProcessModel: actions.updatePostProcessModel,
    fetchPostProcessModels: actions.fetchPostProcessModels,
  };
};
