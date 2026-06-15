// Zustand store for all application settings: audio, shortcuts, TTS, Brain,
// post-processing, VAD, and UI preferences. Syncs bidirectionally with the
// Rust backend via Tauri commands and events.

import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { listen } from "@tauri-apps/api/event";
import type {
  AppSettings as Settings,
  AudioDevice,
  LogLevel,
  WhisperAcceleratorSetting,
  OrtAcceleratorSetting,
  TtsConfig,
  BrainConfig,
} from "@/bindings";
import { commands } from "@/bindings";

interface SettingsStore {
  settings: Settings | null;
  defaultSettings: Settings | null;
  isLoading: boolean;
  /** Guards initialize() so the model-state-changed listener is registered once. */
  initialized: boolean;
  isUpdating: Record<string, boolean>;
  audioDevices: AudioDevice[];
  outputDevices: AudioDevice[];
  customSounds: { start: boolean; stop: boolean };
  postProcessModelOptions: Record<string, string[]>;
  brainModelOptions: Record<string, string[]>;

  // Actions
  initialize: () => Promise<void>;
  loadDefaultSettings: () => Promise<void>;
  updateSetting: <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => Promise<void>;
  resetSetting: (key: keyof Settings) => Promise<void>;
  refreshSettings: () => Promise<void>;
  refreshAudioDevices: () => Promise<void>;
  refreshOutputDevices: () => Promise<void>;
  updateBinding: (id: string, binding: string) => Promise<void>;
  resetBinding: (id: string) => Promise<void>;
  getSetting: <K extends keyof Settings>(key: K) => Settings[K] | undefined;
  isUpdatingKey: (key: string) => boolean;
  playTestSound: (soundType: "start" | "stop") => Promise<void>;
  checkCustomSounds: () => Promise<void>;
  setPostProcessProvider: (providerId: string) => Promise<void>;
  updatePostProcessSetting: (
    settingType: "base_url" | "api_key" | "model",
    providerId: string,
    value: string,
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
  setPostProcessModelOptions: (providerId: string, models: string[]) => void;
  setBrainProvider: (providerId: string) => Promise<void>;
  updateBrainSetting: (
    settingType: "base_url" | "api_key" | "model",
    providerId: string,
    value: string,
  ) => Promise<void>;
  updateBrainBaseUrl: (providerId: string, baseUrl: string) => Promise<void>;
  updateBrainApiKey: (providerId: string, apiKey: string) => Promise<void>;
  updateBrainModel: (providerId: string, model: string) => Promise<void>;
  fetchBrainModels: (providerId: string) => Promise<string[]>;
  setBrainModelOptions: (providerId: string, models: string[]) => void;

  // Internal state setters
  setSettings: (settings: Settings | null) => void;
  setDefaultSettings: (defaultSettings: Settings | null) => void;
  setLoading: (loading: boolean) => void;
  setUpdating: (key: string, updating: boolean) => void;
  setAudioDevices: (devices: AudioDevice[]) => void;
  setOutputDevices: (devices: AudioDevice[]) => void;
  setCustomSounds: (sounds: { start: boolean; stop: boolean }) => void;

  // Internal helpers to deduplicate brain/post-process provider logic
  _setProvider: (prefix: "brain" | "post_process", providerId: string) => Promise<void>;
  _updateProviderSetting: (
    prefix: "brain" | "post_process",
    settingType: "base_url" | "api_key" | "model",
    providerId: string,
    value: string,
  ) => Promise<void>;
  _updateProviderBaseUrl: (prefix: "brain" | "post_process", providerId: string, baseUrl: string) => Promise<void>;
  _fetchProviderModels: (prefix: "brain" | "post_process", providerId: string) => Promise<string[]>;
}

// Note: Default settings are now fetched from Rust via commands.getDefaultSettings()
// This ensures platform-specific defaults (like overlay_position, shortcuts, paste_method) work correctly

const DEFAULT_AUDIO_DEVICE: AudioDevice = {
  index: "default",
  name: "Default",
  is_default: true,
};

const settingUpdaters: {
  [K in keyof Settings]?: (value: Settings[K]) => Promise<unknown>;
} = {
  always_on_microphone: (value) =>
    commands.updateMicrophoneMode(value as boolean),
  audio_feedback: (value) =>
    commands.changeAudioFeedbackSetting(value as boolean),
  audio_feedback_volume: (value) =>
    commands.changeAudioFeedbackVolumeSetting(value as number),
  sound_theme: (value) => commands.changeSoundThemeSetting(value as string),
  start_hidden: (value) => commands.changeStartHiddenSetting(value as boolean),
  autostart_enabled: (value) =>
    commands.changeAutostartSetting(value as boolean),
  update_checks_enabled: (value) =>
    commands.changeUpdateChecksSetting(value as boolean),
  push_to_talk: (value) => commands.changePttSetting(value as boolean),
  selected_microphone: (value) =>
    commands.setSelectedMicrophone(
      (value as string) === "Default" || value === null
        ? "default"
        : (value as string),
    ),
  clamshell_microphone: (value) =>
    commands.setClamshellMicrophone(
      (value as string) === "Default" ? "default" : (value as string),
    ),
  selected_output_device: (value) =>
    commands.setSelectedOutputDevice(
      (value as string) === "Default" || value === null
        ? "default"
        : (value as string),
    ),
  recording_retention_period: (value) =>
    commands.updateRecordingRetentionPeriod(value as string),
  translate_to_english: (value) =>
    commands.changeTranslateToEnglishSetting(value as boolean),
  selected_language: (value) =>
    commands.changeSelectedLanguageSetting(value as string),
  overlay_position: (value) =>
    commands.changeOverlayPositionSetting(value as string),
  debug_mode: (value) => commands.changeDebugModeSetting(value as boolean),
  custom_words: (value) => commands.updateCustomWords(value as string[]),
  word_correction_threshold: (value) =>
    commands.changeWordCorrectionThresholdSetting(value as number),
  paste_delay_ms: (value) =>
    commands.changePasteDelayMsSetting(value as number),
  paste_method: (value) => commands.changePasteMethodSetting(value as string),
  typing_tool: (value) => commands.changeTypingToolSetting(value as string),
  external_script_path: (value) =>
    commands.changeExternalScriptPathSetting(value as string | null),
  clipboard_handling: (value) =>
    commands.changeClipboardHandlingSetting(value as string),
  auto_submit: (value) => commands.changeAutoSubmitSetting(value as boolean),
  auto_submit_key: (value) =>
    commands.changeAutoSubmitKeySetting(value as string),
  history_limit: (value) => commands.updateHistoryLimit(value as number),
  post_process_enabled: (value) =>
    commands.changePostProcessEnabledSetting(value as boolean),
  post_process_selected_prompt_id: (value) =>
    commands.setPostProcessSelectedPrompt(value as string),
  mute_while_recording: (value) =>
    commands.changeMuteWhileRecordingSetting(value as boolean),
  append_trailing_space: (value) =>
    commands.changeAppendTrailingSpaceSetting(value as boolean),
  log_level: (value) => commands.setLogLevel(value as LogLevel),
  app_language: (value) => commands.changeAppLanguageSetting(value as string),
  experimental_enabled: (value) =>
    commands.changeExperimentalEnabledSetting(value as boolean),
  lazy_stream_close: (value) =>
    commands.changeLazyStreamCloseSetting(value as boolean),
  show_tray_icon: (value) =>
    commands.changeShowTrayIconSetting(value as boolean),
  whisper_accelerator: (value) =>
    commands.changeWhisperAcceleratorSetting(
      value as WhisperAcceleratorSetting,
    ),
  ort_accelerator: (value) =>
    commands.changeOrtAcceleratorSetting(value as OrtAcceleratorSetting),
  parakeet_streaming_enabled: (value) =>
    commands.changeParakeetStreamingSetting(value as boolean),
  whisper_gpu_device: (value) =>
    commands.changeWhisperGpuDevice(value as number),
  extra_recording_buffer_ms: (value) =>
    commands.changeExtraRecordingBufferSetting(value as number),
  tts: (value) => commands.changeTtsConfig(value as TtsConfig),
  brain: (value) => commands.changeBrainConfig(value as BrainConfig),
  noise_suppression_enabled: (value) =>
    commands.setNoiseSuppressionEnabled(value as boolean),
  vad_mode: (value) => commands.setVadMode(value as string),
  long_audio_model: (value) =>
    commands.setLongAudioModel(value as string | null),
  long_audio_threshold_seconds: (value) =>
    commands.setLongAudioThreshold(value as number),
};

export const useSettingsStore = create<SettingsStore>()(
  subscribeWithSelector((set, get) => ({
    settings: null,
    defaultSettings: null,
    isLoading: true,
    initialized: false,
    isUpdating: {},
    audioDevices: [],
    outputDevices: [],
    customSounds: { start: false, stop: false },
    postProcessModelOptions: {},
    brainModelOptions: {},

    // Internal setters
    setSettings: (settings) => set({ settings }),
    setDefaultSettings: (defaultSettings) => set({ defaultSettings }),
    setLoading: (isLoading) => set({ isLoading }),
    setUpdating: (key, updating) =>
      set((state) => ({
        isUpdating: { ...state.isUpdating, [key]: updating },
      })),
    setAudioDevices: (audioDevices) => set({ audioDevices }),
    setOutputDevices: (outputDevices) => set({ outputDevices }),
    setCustomSounds: (customSounds) => set({ customSounds }),

    // Getters
    getSetting: (key) => get().settings?.[key],
    isUpdatingKey: (key) => get().isUpdating[key] || false,

    // Load settings from store
    refreshSettings: async () => {
      try {
        const result = await commands.getAppSettings();
        if (result.status === "ok") {
          const settings = result.data;
          const normalizedSettings: Settings = {
            ...settings,
            always_on_microphone: settings.always_on_microphone ?? false,
            selected_microphone: settings.selected_microphone ?? "Default",
            clamshell_microphone: settings.clamshell_microphone ?? "Default",
            selected_output_device:
              settings.selected_output_device ?? "Default",
          };
          set({ settings: normalizedSettings, isLoading: false });
        } else {
          console.error("Failed to load settings:", result.error);
          set({ isLoading: false });
        }
      } catch (error) {
        console.error("Failed to load settings:", error);
        set({ isLoading: false });
      }
    },

    // Load audio devices
    refreshAudioDevices: async () => {
      try {
        const result = await commands.getAvailableMicrophones();
        if (result.status === "ok") {
          const devicesWithDefault = [
            DEFAULT_AUDIO_DEVICE,
            ...result.data.filter(
              (d) => d.name !== "Default" && d.name !== "default",
            ),
          ];
          set({ audioDevices: devicesWithDefault });
        } else {
          set({ audioDevices: [DEFAULT_AUDIO_DEVICE] });
        }
      } catch (error) {
        console.error("Failed to load audio devices:", error);
        set({ audioDevices: [DEFAULT_AUDIO_DEVICE] });
      }
    },

    // Load output devices
    refreshOutputDevices: async () => {
      try {
        const result = await commands.getAvailableOutputDevices();
        if (result.status === "ok") {
          const devicesWithDefault = [
            DEFAULT_AUDIO_DEVICE,
            ...result.data.filter(
              (d) => d.name !== "Default" && d.name !== "default",
            ),
          ];
          set({ outputDevices: devicesWithDefault });
        } else {
          set({ outputDevices: [DEFAULT_AUDIO_DEVICE] });
        }
      } catch (error) {
        console.error("Failed to load output devices:", error);
        set({ outputDevices: [DEFAULT_AUDIO_DEVICE] });
      }
    },

    // Play a test sound
    playTestSound: async (soundType: "start" | "stop") => {
      try {
        await commands.playTestSound(soundType);
      } catch (error) {
        console.error(`Failed to play test sound (${soundType}):`, error);
      }
    },

    checkCustomSounds: async () => {
      try {
        const sounds = await commands.checkCustomSounds();
        get().setCustomSounds(sounds);
      } catch (error) {
        console.error("Failed to check custom sounds:", error);
      }
    },

    // Update a specific setting
    updateSetting: async <K extends keyof Settings>(
      key: K,
      value: Settings[K],
    ) => {
      const { settings, setUpdating } = get();
      const updateKey = String(key);
      const originalValue = settings?.[key];

      setUpdating(updateKey, true);

      try {
        set((state) => ({
          settings: state.settings ? { ...state.settings, [key]: value } : null,
        }));

        const updater = settingUpdaters[key];
        if (updater) {
          await updater(value);
        } else if (key !== "bindings" && key !== "selected_model") {
          console.warn(`No handler for setting: ${String(key)}`);
        }
      } catch (error) {
        console.error(`Failed to update setting ${String(key)}:`, error);
        if (settings) {
          set({ settings: { ...settings, [key]: originalValue } });
        }
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Reset a setting to its default value
    resetSetting: async (key) => {
      const { defaultSettings } = get();
      if (defaultSettings) {
        const defaultValue = defaultSettings[key];
        if (defaultValue !== undefined) {
          await get().updateSetting(key, defaultValue as any);
        }
      }
    },

    // Update a specific binding
    updateBinding: async (id, binding) => {
      const { settings, setUpdating } = get();
      const updateKey = `binding_${id}`;
      const originalBinding = settings?.bindings?.[id]?.current_binding;

      setUpdating(updateKey, true);

      try {
        // Optimistic update
        set((state) => ({
          settings: state.settings
            ? {
                ...state.settings,
                bindings: {
                  ...state.settings.bindings,
                  [id]: {
                    ...state.settings.bindings[id]!,
                    current_binding: binding,
                  },
                },
              }
            : null,
        }));

        const result = await commands.changeBinding(id, binding);

        // Check if the command executed successfully
        if (result.status === "error") {
          throw new Error(result.error);
        }

        // Check if the binding change was successful
        if (!result.data.success) {
          throw new Error(result.data.error || "Failed to update binding");
        }
      } catch (error) {
        console.error(`Failed to update binding ${id}:`, error);

        // Rollback on error
        if (originalBinding && get().settings) {
          set((state) => ({
            settings: state.settings
              ? {
                  ...state.settings,
                  bindings: {
                    ...state.settings.bindings,
                    [id]: {
                      ...state.settings.bindings[id]!,
                      current_binding: originalBinding,
                    },
                  },
                }
              : null,
          }));
        }

        // Re-throw to let the caller know it failed
        throw error;
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Reset a specific binding
    resetBinding: async (id) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `binding_${id}`;

      setUpdating(updateKey, true);

      try {
        await commands.resetBinding(id);
        await refreshSettings();
      } catch (error) {
        console.error(`Failed to reset binding ${id}:`, error);
      } finally {
        setUpdating(updateKey, false);
      }
    },

    setPostProcessProvider: async (providerId) => {
      await get()._setProvider("post_process", providerId);
    },

    updatePostProcessSetting: async (
      settingType: "base_url" | "api_key" | "model",
      providerId: string,
      value: string,
    ) => {
      await get()._updateProviderSetting("post_process", settingType, providerId, value);
    },

    updatePostProcessBaseUrl: async (providerId, baseUrl) => {
      await get()._updateProviderBaseUrl("post_process", providerId, baseUrl);
    },

    updatePostProcessApiKey: async (providerId, apiKey) => {
      set((state) => ({
        postProcessModelOptions: {
          ...state.postProcessModelOptions,
          [providerId]: [],
        },
      }));
      return get().updatePostProcessSetting("api_key", providerId, apiKey);
    },

    updatePostProcessModel: async (providerId, model) => {
      return get().updatePostProcessSetting("model", providerId, model);
    },

    fetchPostProcessModels: async (providerId) => {
      return get()._fetchProviderModels("post_process", providerId);
    },

    setPostProcessModelOptions: (providerId, models) =>
      set((state) => ({
        postProcessModelOptions: {
          ...state.postProcessModelOptions,
          [providerId]: models,
        },
      })),

    setBrainProvider: async (providerId) => {
      await get()._setProvider("brain", providerId);
    },

    updateBrainSetting: async (
      settingType: "base_url" | "api_key" | "model",
      providerId: string,
      value: string,
    ) => {
      await get()._updateProviderSetting("brain", settingType, providerId, value);
    },

    updateBrainBaseUrl: async (providerId, baseUrl) => {
      await get()._updateProviderBaseUrl("brain", providerId, baseUrl);
    },

    updateBrainApiKey: async (providerId, apiKey) => {
      set((state) => ({
        brainModelOptions: {
          ...state.brainModelOptions,
          [providerId]: [],
        },
      }));
      return get().updateBrainSetting("api_key", providerId, apiKey);
    },

    updateBrainModel: async (providerId, model) => {
      return get().updateBrainSetting("model", providerId, model);
    },

    fetchBrainModels: async (providerId) => {
      return get()._fetchProviderModels("brain", providerId);
    },

    setBrainModelOptions: (providerId, models) =>
      set((state) => ({
        brainModelOptions: {
          ...state.brainModelOptions,
          [providerId]: models,
        },
      })),

    _setProvider: async (prefix, providerId) => {
      const { settings, setUpdating, refreshSettings } = get();
      const updateKey = `${prefix}_provider_id`;
      const previousId =
        prefix === "brain"
          ? settings?.brain?.provider_id ?? null
          : settings?.post_process_provider_id ?? null;

      setUpdating(updateKey, true);

      if (settings) {
        if (prefix === "brain" && settings.brain) {
          set((state) => ({
            settings: state.settings
              ? {
                  ...state.settings,
                  brain: { ...state.settings.brain!, provider_id: providerId },
                }
              : null,
          }));
        } else {
          set((state) => ({
            settings: state.settings
              ? { ...state.settings, post_process_provider_id: providerId }
              : null,
          }));
        }
      }

      // Clear cached model options
      const clearFn =
        prefix === "brain"
          ? get().setBrainModelOptions
          : get().setPostProcessModelOptions;
      clearFn(providerId, []);

      try {
        const cmd =
          prefix === "brain"
            ? commands.setBrainProvider(providerId)
            : commands.setPostProcessProvider(providerId);
        await cmd;
        await refreshSettings();
      } catch (error) {
        console.error(`Failed to set ${prefix} provider:`, error);
        if (previousId !== null) {
          if (prefix === "brain" && settings?.brain) {
            set((state) => ({
              settings: state.settings
                ? {
                    ...state.settings,
                    brain: {
                      ...state.settings.brain!,
                      provider_id: previousId,
                    },
                  }
                : null,
            }));
          } else {
            set((state) => ({
              settings: state.settings
                ? {
                    ...state.settings,
                    post_process_provider_id: previousId,
                  }
                : null,
            }));
          }
        }
      } finally {
        setUpdating(updateKey, false);
      }
    },

    _updateProviderSetting: async (
      prefix,
      settingType,
      providerId,
      value,
    ) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `${prefix}_${settingType}:${providerId}`;

      setUpdating(updateKey, true);

      try {
        const cmds = {
          base_url:
            prefix === "brain"
              ? commands.changeBrainBaseUrlSetting(providerId, value)
              : commands.changePostProcessBaseUrlSetting(providerId, value),
          api_key:
            prefix === "brain"
              ? commands.changeBrainApiKeySetting(providerId, value)
              : commands.changePostProcessApiKeySetting(providerId, value),
          model:
            prefix === "brain"
              ? commands.changeBrainModelSetting(providerId, value)
              : commands.changePostProcessModelSetting(providerId, value),
        };
        await cmds[settingType];
        await refreshSettings();
      } catch (error) {
        console.error(
          `Failed to update ${prefix} ${settingType.replace("_", " ")}:`,
          error,
        );
      } finally {
        setUpdating(updateKey, false);
      }
    },

    _updateProviderBaseUrl: async (prefix, providerId, baseUrl) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `${prefix}_base_url:${providerId}`;

      setUpdating(updateKey, true);

      try {
        const urlCmd =
          prefix === "brain"
            ? commands.changeBrainBaseUrlSetting(providerId, baseUrl)
            : commands.changePostProcessBaseUrlSetting(providerId, baseUrl);
        const urlResult = await urlCmd;
        if (urlResult.status === "error") {
          console.error("Failed to persist base URL:", urlResult.error);
          return;
        }

        const modelCmd =
          prefix === "brain"
            ? commands.changeBrainModelSetting(providerId, "")
            : commands.changePostProcessModelSetting(providerId, "");
        const modelResult = await modelCmd;
        if (modelResult.status === "error") {
          console.error("Failed to reset model setting:", modelResult.error);
          return;
        }

        const clearFn =
          prefix === "brain"
            ? get().setBrainModelOptions
            : get().setPostProcessModelOptions;
        clearFn(providerId, []);

        await refreshSettings();
      } catch (error) {
        console.error(`Failed to update ${prefix} base URL:`, error);
      } finally {
        setUpdating(updateKey, false);
      }
    },

    _fetchProviderModels: async (prefix, providerId) => {
      const updateKey = `${prefix}_models_fetch:${providerId}`;
      const { setUpdating } = get();
      const setFn =
        prefix === "brain"
          ? get().setBrainModelOptions
          : get().setPostProcessModelOptions;

      setUpdating(updateKey, true);

      try {
        const cmd =
          prefix === "brain"
            ? commands.fetchBrainModels()
            : commands.fetchPostProcessModels(providerId);
        const result = await cmd;
        if (result.status === "ok") {
          setFn(providerId, result.data);
          return result.data;
        } else {
          console.error(`Failed to fetch ${prefix} models:`, result.error);
          return [];
        }
      } catch (error) {
        console.error(`Failed to fetch ${prefix} models:`, error);
        return [];
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Load default settings from Rust
    loadDefaultSettings: async () => {
      try {
        const result = await commands.getDefaultSettings();
        if (result.status === "ok") {
          set({ defaultSettings: result.data });
        } else {
          console.error("Failed to load default settings:", result.error);
        }
      } catch (error) {
        console.error("Failed to load default settings:", error);
      }
    },

    // Initialize everything
    initialize: async () => {
      // Guard against the many components that call initialize() during the initial
      // async load window — without this, each one registers another
      // `model-state-changed` listener that is never cleaned up.
      if (get().initialized) return;
      set({ initialized: true });

      const { refreshSettings, checkCustomSounds, loadDefaultSettings } = get();

      // Note: Audio devices are NOT refreshed here. The frontend (App.tsx)
      // is responsible for calling refreshAudioDevices/refreshOutputDevices
      // after onboarding completes. This avoids triggering permission dialogs
      // on macOS before the user is ready.
      await Promise.all([
        loadDefaultSettings(),
        refreshSettings(),
        checkCustomSounds(),
      ]);

      // Re-fetch settings when the backend changes them (e.g. language
      // reset during model switch). The backend is the source of truth.
      listen("model-state-changed", () => {
        get().refreshSettings();
      });
    },
  })),
);
