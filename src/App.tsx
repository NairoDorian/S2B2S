import {
  createSignal,
  createEffect,
  onCleanup,
  onSettled,
  Switch,
  Match,
  Show,
} from "solid-js";
import { Toaster } from "@/components/ui/Toaster";
import { sessionToast as toast } from "@/lib/sessionToast";
import { useTranslation, currentLanguage } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { ModelStateEvent, RecordingErrorEvent } from "./lib/types/events";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import SecureInputWarning from "./components/SecureInputWarning";
import Footer from "./components/footer";
import Onboarding, { AccessibilityOnboarding } from "./components/onboarding";
import {
  DebugSettings,
  type OnboardingPreviewStep,
} from "./components/settings";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { HotkeySidebar } from "./components/hotkey-sidebar";
import { QuickHelp } from "./components/settings/QuickHelp";
import { useNavigationStore, setSection } from "@/stores/navigationStore";
import { WhatsNewGate } from "./components/whats-new";
import { useSettings } from "./hooks/useSettings";
import { commands, events } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";
import { applyUiScale } from "@/lib/utils/theme";

type OnboardingStep = "accessibility" | "model" | "done";

const NOOP = () => {};

let hasCompletedPostOnboardingInit = false;

const currentSection = () => useNavigationStore().section;
const direction = () => getLanguageDirection(currentLanguage());

const revealMainWindowForPermissions = async () => {
  try {
    await commands.showMainWindowCommand();
  } catch (e) {
    console.warn("Failed to show main window for permission onboarding:", e);
  }
};

const renderSettingsContent = (
  section: SidebarSection,
  onPreviewOnboarding: (step: OnboardingPreviewStep) => void,
) => {
  if (section === "debug") {
    return <DebugSettings onPreviewOnboarding={onPreviewOnboarding} />;
  }

  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

function App() {
  const { t } = useTranslation();
  const [onboardingStep, setOnboardingStep] =
    createSignal<OnboardingStep | null>(null);
  const [onboardingPreview, setOnboardingPreview] =
    createSignal<OnboardingPreviewStep | null>(null);
  const [isReturningUser, setIsReturningUser] = createSignal(false);
  const { settings, updateSetting, refreshAudioDevices, refreshOutputDevices } =
    useSettings();
  const uiScale = () => settings()?.ui_scale ?? null;

  createEffect(
    () => uiScale(),
    (scale) => {
      if (scale !== null) applyUiScale(scale);
    },
  );
  const isShowingOnboarding = () =>
    onboardingPreview() !== null ||
    onboardingStep() === "accessibility" ||
    onboardingStep() === "model";

  createEffect(
    () => isShowingOnboarding(),
    (showing) => {
      document.documentElement.toggleAttribute(
        "data-onboarding-active",
        showing,
      );
    },
  );

  onSettled(() => {
    checkOnboardingStatus();
  });

  createEffect(
    () => currentLanguage(),
    (language) => {
      initializeRTL(language);
    },
  );

  // Tracked on `onboardingStep`, not run once: this used to fire at mount, when
  // the step is still `null`, so the Enigo/shortcut init and the device refresh
  // never ran at all.
  createEffect(
    () => onboardingStep(),
    (step) => {
      if (step === "done" && !hasCompletedPostOnboardingInit) {
        hasCompletedPostOnboardingInit = true;
        Promise.all([
          commands.initializeEnigo(),
          commands.initializeShortcuts(),
        ]).catch((e) => {
          console.warn("Failed to initialize:", e);
        });
        refreshAudioDevices();
        refreshOutputDevices();
      }
    },
  );

  createEffect(
    () => undefined,
    () => {
      const handleKeyDown = (event: KeyboardEvent) => {
        const isDebugShortcut =
          event.shiftKey &&
          event.key.toLowerCase() === "d" &&
          (event.ctrlKey || event.metaKey);

        if (isDebugShortcut) {
          event.preventDefault();
          const currentDebugMode = settings()?.debug_mode ?? false;
          updateSetting("debug_mode", !currentDebugMode);
        }
      };

      document.addEventListener("keydown", handleKeyDown);
      onCleanup(() => {
        document.removeEventListener("keydown", handleKeyDown);
      });
    },
  );

  createEffect(
    () => currentLanguage(),
    () => {
      const unlisten = listen<RecordingErrorEvent>(
        "recording-error",
        (event) => {
          const { error_type, detail } = event.payload;

          if (error_type === "microphone_permission_denied") {
            const currentPlatform = platform();
            const platformKey = `errors.micPermissionDenied.${currentPlatform}`;
            const description = t(platformKey, {
              defaultValue: t("errors.micPermissionDenied.generic"),
            });
            toast.error(t("errors.micPermissionDeniedTitle"), { description });
          } else if (error_type === "no_input_device") {
            toast.error(t("errors.noInputDeviceTitle"), {
              description: t("errors.noInputDevice"),
            });
          } else {
            toast.error(
              t("errors.recordingFailed", { error: detail ?? "Unknown error" }),
            );
          }
        },
      );
      onCleanup(() => {
        unlisten.then((fn) => fn());
      });
    },
  );

  createEffect(
    () => currentLanguage(),
    () => {
      const unlisten = listen("paste-error", () => {
        toast.error(t("errors.pasteFailedTitle"), {
          description: t("errors.pasteFailed"),
        });
      });
      onCleanup(() => {
        unlisten.then((fn) => fn());
      });
    },
  );

  createEffect(
    () => currentLanguage(),
    () => {
      const unlisten = listen<string>("transcription-error", (event) => {
        toast.error(t("errors.transcriptionFailedTitle"), {
          description: event.payload,
        });
      });
      onCleanup(() => {
        unlisten.then((fn) => fn());
      });
    },
  );

  createEffect(
    () => currentLanguage(),
    () => {
      const unlisten = listen<ModelStateEvent>(
        "model-state-changed",
        (event) => {
          if (event.payload.event_type === "loading_failed") {
            toast.error(
              t("errors.modelLoadFailed", {
                model:
                  event.payload.model_name ||
                  t("errors.modelLoadFailedUnknown"),
              }),
              {
                description: event.payload.error,
              },
            );
          }
          if (event.payload.event_type === "multi_stt_model_load_failed") {
            toast.error(
              t("errors.modelLoadFailed", {
                model:
                  event.payload.model_id || t("errors.modelLoadFailedUnknown"),
              }),
              {
                description: event.payload.error,
              },
            );
          }
        },
      );
      onCleanup(() => {
        unlisten.then((fn) => fn());
      });
    },
  );

  createEffect(
    () => currentLanguage(),
    () => {
      const unlisten = events.multiSttStreamChunkFailedEvent.listen((event) => {
        toast.warning(t("multiStt.streamingFirst.chunkFailedTitle"), {
          description: t("multiStt.streamingFirst.chunkFailedToast", {
            chunk: event.payload.chunk,
          }),
        });
      });
      onCleanup(() => {
        unlisten.then((fn) => fn());
      });
    },
  );

  const checkOnboardingStatus = async () => {
    try {
      const settingsResult = await commands.getAppSettings();
      const hasCompletedOnboarding =
        settingsResult.status === "ok" &&
        settingsResult.data.onboarding_completed === true;
      const currentPlatform = platform();

      if (hasCompletedOnboarding) {
        setIsReturningUser(true);

        if (currentPlatform === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check macOS permissions:", e);
          }
        }

        if (currentPlatform === "windows") {
          try {
            const microphoneStatus =
              await commands.getWindowsMicrophonePermissionStatus();
            if (
              microphoneStatus.supported &&
              microphoneStatus.overall_access === "denied"
            ) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check Windows microphone permissions:", e);
          }
        }

        setOnboardingStep("done");
      } else {
        setIsReturningUser(false);
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleAccessibilityComplete = () => {
    setOnboardingStep(isReturningUser() ? "done" : "model");
  };

  const handleModelSelected = () => {
    setOnboardingStep("done");
  };

  const toaster = <Toaster />;

  // A `Switch` rather than an `if/else` chain assigning to a local: a Solid
  // component body runs once, so `let content; if (step() === null) …` would
  // snapshot the pre-load `null` and never render the shell. `Match` re-reads
  // its `when` reactively, so the window appears as soon as onboarding status
  // resolves.
  return (
    <>
      {toaster}
      <Switch>
        <Match when={onboardingStep() !== null && onboardingPreview() !== null}>
          <Show
            when={onboardingPreview() === "accessibility"}
            fallback={<Onboarding onModelSelected={NOOP} preview />}
          >
            <AccessibilityOnboarding onComplete={NOOP} preview />
          </Show>
          <button
            type="button"
            onClick={() => setOnboardingPreview(null)}
            class="fixed top-4 end-4 z-50 rounded-lg border border-mid-gray/20 bg-background px-4 py-2 text-sm font-medium text-text shadow-lg hover:bg-background-ui/30 cursor-pointer"
          >
            {t("settings.debug.onboardingPreview.exitButton")}
          </button>
        </Match>
        <Match when={onboardingStep() === "accessibility"}>
          <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />
        </Match>
        <Match when={onboardingStep() === "model"}>
          <Onboarding onModelSelected={handleModelSelected} />
        </Match>
        <Match when={onboardingStep() === "done"}>
          <div
            dir={direction()}
            class="h-screen flex flex-col select-none cursor-default"
          >
            <ErrorBoundary context="What's New">
              <WhatsNewGate />
            </ErrorBoundary>
            <div class="flex-1 flex overflow-hidden">
              <Sidebar
                activeSection={currentSection()}
                onSectionChange={setSection}
              />
              <div class="flex-1 flex flex-col overflow-hidden">
                <div class="flex-1 overflow-y-auto">
                  <div class="flex flex-col items-center p-4 pe-14 gap-4">
                    <AccessibilityPermissions />
                    <SecureInputWarning />
                    <QuickHelp activeSection={currentSection()} />
                    {renderSettingsContent(
                      currentSection(),
                      setOnboardingPreview,
                    )}
                  </div>
                </div>
              </div>
              <HotkeySidebar />
            </div>
            <Footer />
          </div>
        </Match>
      </Switch>
    </>
  );
}

export default App;
