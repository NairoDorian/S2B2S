import { Page } from "@playwright/test";

/**
 * Injects the core mock Tauri IPC layer and OS/event structures.
 */
export async function mockTauriIpc(page: Page) {
  await page.addInitScript(() => {
    // 1. Mock window.__TAURI_OS_PLUGIN_INTERNALS__ for @tauri-apps/plugin-os
    (window as any).__TAURI_OS_PLUGIN_INTERNALS__ = {
      platform: "linux",
      version: "22.04",
      family: "unix",
      os_type: "linux",
      arch: "x86_64",
      exe_extension: "",
      eol: "\n",
    };

    // 2. Initialize a global registry for custom command handlers and events
    (window as any).__mockHandlers = {};
    (window as any).__eventMap = {};
    (window as any).__downloadedModels = [];

    // 3. Mock window.__TAURI_INTERNALS__ without destroying other properties
    const internals = (window as any).__TAURI_INTERNALS__ || {};

    // Define transformCallback
    internals.transformCallback =
      internals.transformCallback ||
      ((callback: any, once: boolean) => {
        const identifier = Math.floor(Math.random() * 9007199254740991) + 1;
        (window as any)[identifier] = (data: any) => {
          if (once) {
            delete (window as any)[identifier];
          }
          callback(data);
        };
        return identifier;
      });

    // Mock window.__TAURI_EVENT_PLUGIN_INTERNALS__ for @tauri-apps/api/event
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = (window as any)
      .__TAURI_EVENT_PLUGIN_INTERNALS__ || {
      unregisterListener(event: string, id: number) {
        if ((window as any).__eventMap && (window as any).__eventMap[event]) {
          (window as any).__eventMap[event] = (window as any).__eventMap[
            event
          ].filter((cbId: number) => cbId !== id);
        }
      },
    };

    // Mock legacy Tauri event system registry
    internals.event = internals.event || {
      listeners: {},
      async registerListener(event: string, id: number, handler: Function) {
        if (!this.listeners[event]) {
          this.listeners[event] = [];
        }
        this.listeners[event].push({ id, handler });
      },
      async unregisterListener(event: string, id: number) {
        if (this.listeners[event]) {
          this.listeners[event] = this.listeners[event].filter(
            (item: any) => item.id !== id,
          );
        }
      },
    };

    internals.invoke = async (cmd: string, args: any) => {
      // Check custom handler registry first
      const customHandler = (window as any).__mockHandlers?.[cmd];
      if (customHandler !== undefined) {
        if (typeof customHandler === "function") {
          return customHandler(args);
        }
        return customHandler;
      }

      // Generic mock fallbacks for S2B2S commands
      switch (cmd) {
        case "get_app_settings":
        case "get_default_settings":
          return {
            push_to_talk: false,
            audio_feedback: false,
            audio_feedback_volume: 0.5,
            sound_theme: "default",
            start_hidden: false,
            autostart_enabled: false,
            update_checks_enabled: false,
            selected_model: "Parakeet V3",
            always_on_microphone: false,
            selected_microphone: "Default System Mic",
            selected_output_device: "Default Speaker",
            translate_to_english: false,
            selected_language: "en-US",
            overlay_position: "BottomRight",
            overlay_window: { enabled: true },
            wgpu_trail: { enabled: false },
            debug_mode: false,
            log_level: "Info",
            custom_words: [],
            model_unload_timeout: "never",
            word_correction_threshold: 0.2,
            history_limit: 100,
            recording_retention_period: "never",
            paste_method: "clipboard",
            clipboard_handling: "paste",
            auto_submit: false,
            auto_submit_key: "Enter",
            post_process_enabled: false,
            post_process_provider_id: "llama_cpp",
            post_process_providers: [],
            post_process_api_keys: {},
            post_process_models: {},
            post_process_prompts: [],
            llm_models: [],
            post_process_actions: [],
            post_process_actions_initialized: true,
            mute_while_recording: false,
            append_trailing_space: false,
            app_language: "en",
            experimental_enabled: false,
            lazy_stream_close: false,
            keyboard_implementation: "enigo",
            show_tray_icon: true,
            paste_delay_ms: 100,
            typing_tool: "enigo",
            external_script_path: null,
            custom_filler_words: [],
            whisper_accelerator: "cpu",
            ort_accelerator: "cpu",
            whisper_gpu_device: 0,
            extra_recording_buffer_ms: 500,
            tts: {
              enabled: true,
              engine: "piper",
              voice: "en_US-lessac-medium",
              speed: 1.0,
              volume: 80,
              pagination: { enabled: true },
              sanitization: {
                enabled: true,
                markdown: true,
                tts_normalization: true,
              },
              piper: { model_id: "en_US-lessac-medium", voice_id: "medium" },
            },
            brain: {
              enabled: true,
              provider_id: "llama_cpp",
              providers: [
                {
                  id: "llama_cpp",
                  label: "Llama.cpp",
                  base_url: "http://localhost:8080/v1",
                },
              ],
              api_keys: {},
              models: {
                llama_cpp: "Gemma-4 2B (Local)",
              },
              system_prompt: "You are a helpful assistant.",
              context_turns: 20,
              read_aloud: true,
              speakable_output_prompt: "",
              warmup_prompt: "",
              conversation_mode: "push_to_talk",
              endpoint_preset: "balanced",
              headphone_mode: false,
              auto_listen: false,
              multimodal_audio_enabled: false,
            },
            long_audio_model: null,
            long_audio_threshold_seconds: 30,
            noise_suppression_enabled: true,
            vad_mode: "TripleVAD",
            rnnoise_voice_threshold: 0.2,
            llama_server: { backend: "cpu", release_tag: "" },
            first_run: false,
          };
        case "check_speech_runtime_installed":
          return false;
        case "install_speech_runtime":
          // Simulates installation progress steps
          setTimeout(() => {
            (window as any).__mockEmit("runtime-install-progress", {
              message: "[1/5] Downloading portable uv...",
            });
            setTimeout(() => {
              (window as any).__mockEmit("runtime-install-progress", {
                message: "[3/5] Creating standalone virtual environment...",
              });
              setTimeout(() => {
                (window as any).__mockEmit("runtime-install-progress", {
                  message: "[4/5] Installing dependencies...",
                });
                setTimeout(() => {
                  (window as any).__mockEmit("runtime-install-success", {});
                }, 200);
              }, 200);
            }, 200);
          }, 100);
          return null;
        case "has_any_models_available":
          return ((window as any).__downloadedModels || []).length > 0;
        case "get_models":
        case "get_available_models":
          const mockModels = [
            {
              id: "parakeet-v3",
              name: "Parakeet V3",
              description: "Recommended high accuracy speech-to-text model",
              filename: "parakeet-v3.onnx",
              url: "http://example.com/parakeet-v3.onnx",
              sha256: null,
              size_mb: 478,
              is_downloaded: false,
              is_downloading: false,
              partial_size: null,
              is_directory: false,
              engine_type: "sherpa-onnx",
              accuracy_score: 4.8,
              speed_score: 4.5,
              supports_translation: false,
              is_recommended: true,
              supported_languages: ["en"],
              supports_language_selection: true,
              is_custom: false,
            },
            {
              id: "gemma-2b",
              name: "Gemma-4 2B (Local)",
              description: "Fast local language model",
              filename: "gemma-2b.gguf",
              url: "http://example.com/gemma-2b.gguf",
              sha256: null,
              size_mb: 1300,
              is_downloaded: false,
              is_downloading: false,
              partial_size: null,
              is_directory: false,
              engine_type: "sherpa-onnx",
              accuracy_score: 4.0,
              speed_score: 4.2,
              supports_translation: false,
              is_recommended: false,
              supported_languages: ["en"],
              supports_language_selection: false,
              is_custom: false,
            },
          ];
          return mockModels.map((m) => ({
            ...m,
            is_downloaded: ((window as any).__downloadedModels || []).includes(
              m.id,
            ),
          }));
        case "get_current_model":
          return null;
        case "get_transcription_model_status":
          return ((window as any).__downloadedModels || []).includes(
            "parakeet-v3",
          )
            ? "parakeet-v3"
            : null;
        case "is_model_loading":
          return false;
        case "download_model":
          const modelId = args.modelId || "parakeet-v3";
          setTimeout(() => {
            (window as any).__mockEmit("model-download-progress", {
              model_id: modelId,
              percentage: 25,
              speed: 12.5,
            });
            setTimeout(() => {
              (window as any).__mockEmit("model-download-progress", {
                model_id: modelId,
                percentage: 75,
                speed: 15.0,
              });
              setTimeout(() => {
                if (!(window as any).__downloadedModels.includes(modelId)) {
                  (window as any).__downloadedModels.push(modelId);
                }
                (window as any).__mockEmit("model-download-complete", modelId);
                (window as any).__mockEmit("model-state-changed", {
                  event_type: "downloaded",
                  model_id: modelId,
                  model_name: modelId,
                });
              }, 200);
            }, 200);
          }, 100);
          return true;
        case "select_model":
        case "set_active_model":
          return true;
        case "initialize_enigo":
          return true;
        case "initialize_shortcuts":
          return true;
        case "check_mic_permission":
          return true;
        case "get_windows_microphone_permission_status":
          return { supported: true, overall_access: "granted" };
        case "get_system_ram":
          return { total_mb: 16384, used_mb: 8192, free_mb: 8192 };
        case "save_settings":
          return true;
        case "check_custom_sounds":
          return [];
        case "get_available_microphones":
          return ["Default System Mic"];
        case "get_available_output_devices":
          return ["Default Speaker"];
        case "plugin:os|locale":
          return "en-US";
        case "plugin:event|listen": {
          const { event, handler } = args;
          if (!(window as any).__eventMap[event]) {
            (window as any).__eventMap[event] = [];
          }
          (window as any).__eventMap[event].push(handler);
          return handler;
        }
        case "plugin:event|unlisten": {
          const { event, eventId } = args;
          if ((window as any).__eventMap[event]) {
            (window as any).__eventMap[event] = (window as any).__eventMap[
              event
            ].filter((cbId: number) => cbId !== eventId);
          }
          return null;
        }
        case "get_piper_server_status":
          return { running: false, ready: false, model: null, cuda: false };
        case "tts_get_voices":
          return [
            {
              id: "en_US-lessac-medium",
              name: "Lessac (medium)",
              language: "en-US",
            },
          ];
        case "get_active_gpu_vram_status":
          return {
            is_supported: false,
            adapter_name: null,
            total_vram_mb: 0,
            used_vram_mb: 0,
            free_vram_mb: 0,
            process_used_mb: 0,
            process_budget_mb: 0,
            llm_servers: [],
            updated_at_unix_ms: null,
            error: null,
          };
        case "plugin:app|version":
          return "1.0.0";
        case "stop_continuous_voice_mode":
          return null;
        default:
          console.warn(`[Tauri Mock] Unhandled invoke command: ${cmd}`, args);
          return null;
      }
    };

    (window as any).__TAURI_INTERNALS__ = internals;

    // Helper to let test runner emit events into the frontend
    (window as any).__mockEmit = (event: string, payload: any) => {
      // Trigger eventMap listeners
      const list = (window as any).__eventMap?.[event] || [];
      for (const callbackId of list) {
        const cb = (window as any)[callbackId];
        if (typeof cb === "function") {
          cb({ event, id: callbackId, payload });
        }
      }
      // Also trigger legacy internals.event list if any exist
      const legacyList = internals.event?.listeners?.[event] || [];
      for (const item of legacyList) {
        item.handler({ event, payload });
      }
    };
  });
}

/**
 * Helper to emit a custom event to the frontend (e.g. STT transcriptions)
 */
export async function emitMockEvent(page: Page, event: string, payload: any) {
  await page.evaluate(
    ({ event, payload }) => {
      (window as any).__mockEmit(event, payload);
    },
    { event, payload },
  );
}
