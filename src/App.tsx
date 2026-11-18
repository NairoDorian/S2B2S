import {
  createSignal,
  createEffect,
  onSettled,
  Switch,
  Match,
  Show,
} from "solid-js";
import { Toaster } from "@/components/ui/Toaster";
import { sessionToast as toast } from "@/lib/sessionToast";
import { useTranslation, currentLanguage } from "@/i18n/useTranslation";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import {
  isSupportedAudioPath,
  useFileTranscriptionStore,
} from "@/stores/fileTranscriptionStore";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import SecureInputWarning from "./components/SecureInputWarning";
import Footer from "./components/footer";
import Onboarding, {
  AccessibilityOnboarding,
  LanguagesOnboarding,
} from "./components/onboarding";
import {
  DebugSettings,
  type OnboardingPreviewStep,
} from "./components/settings";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { HotkeySidebar } from "./components/hotkey-sidebar";
import { QuickHelp } from "./components/settings/QuickHelp";
import {
  useNavigationStore,
  setSection,
  setLiveCanvas,
} from "@/stores/navigationStore";
import { LiveCanvas } from "@/components/live-canvas/LiveCanvas";
import { WhatsNewGate } from "./components/whats-new";
import { useSettings } from "./hooks/useSettings";
import { commands, events } from "@/bindings";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { applyUiScale } from "@/lib/utils/theme";
import { applyTypography, normalizeTypography } from "@/lib/typography";

type OnboardingStep = "accessibility" | "model" | "languages" | "done";

const NOOP = () => {};

let hasCompletedPostOnboardingInit = false;

const currentSection = () => useNavigationStore().section;

/**
 * Audio dropped anywhere in the window opens Transcribe Files with it queued
 * (upstream `73c0958` / `815e73f`). On that page its own handler takes the
 * drop (folders included), and nothing is queued while a job runs there.
 */
const routeDroppedAudio = (paths: string[]) => {
  if (currentSection() === "fileTranscription") return;
  const audio = paths.filter(isSupportedAudioPath);
  if (audio.length === 0) return;
  setSection("fileTranscription");
  const files = useFileTranscriptionStore();
  if (!files.running) files.addPaths(audio);
};
const direction = () => getLanguageDirection(currentLanguage());

const revealMainWindowForPermissions = async () => {
  try {
    const result = await commands.showMainWindowCommand();
    if (result.status === "error") {
      console.warn(
        "Failed to show main window for permission onboarding:",
        result.error,
      );
    }
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
    () => {
      const current = settings();
      return current ? normalizeTypography(current.typography) : null;
    },
    (fonts) => {
      if (fonts) applyTypography(fonts);
    },
  );

  createEffect(
    () => uiScale(),
    (scale) => {
      if (scale !== null) applyUiScale(scale);
    },
  );
  const isShowingOnboarding = () =>
    onboardingPreview() !== null ||
    onboardingStep() === "accessibility" ||
    onboardingStep() === "model" ||
    onboardingStep() === "languages";

  let settingsScrollRef: HTMLDivElement | undefined;

  // Reset the scroll position whenever the active section changes.
  createEffect(
    () => currentSection(),
    () => {
      settingsScrollRef?.scrollTo({ top: 0 });
    },
  );

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
        ])
          .then(([enigo, shortcuts]) => {
            if (enigo.status === "error") {
              console.warn("Failed to initialize Enigo:", enigo.error);
            }
            if (shortcuts.status === "error") {
              console.warn("Failed to initialize shortcuts:", shortcuts.error);
            }
          })
          .catch((e) => {
            console.warn("Failed to initialize:", e);
          });
        refreshAudioDevices();
        refreshOutputDevices();
      }
    },
  );

  // Each subscription below runs once (`() => undefined` tracks nothing) and
  // returns its cleanup from the apply: `onCleanup` there has no owner and
  // never runs. None of them depends on the language: `t()` resolves against
  // the current language when a handler calls it.
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
      return () => {
        document.removeEventListener("keydown", handleKeyDown);
      };
    },
  );

  // Recording, paste, transcription and model-load failures are notices
  // (`src-tauri/src/notices.rs`): the footer chip lists them and pops each
  // one up as a toast (`stores/noticeStore.ts`).

  createEffect(
    () => undefined,
    () => {
      let disposed = false;
      let unlisten: (() => void) | null = null;
      getCurrentWebviewWindow()
        .onDragDropEvent((event) => {
          if (event.payload.type === "drop")
            routeDroppedAudio(event.payload.paths);
        })
        .then((fn) => {
          if (disposed) fn();
          else unlisten = fn;
        })
        .catch((error) =>
          console.error("Drag & drop listener unavailable:", error),
        );
      return () => {
        disposed = true;
        unlisten?.();
      };
    },
  );
  createEffect(
    () => undefined,
    () => {
      const unlisten = events.multiSttStreamChunkFailedEvent.listen((event) => {
        toast.warning(t("multiStt.streamingFirst.chunkFailedTitle"), {
          description: t("multiStt.streamingFirst.chunkFailedToast", {
            chunk: event.payload.chunk,
          }),
        });
      });
      return () => {
        unlisten.then((fn) => fn());
      };
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

  // A first run asks for the spoken languages after the model; a returning
  // user (re-shown onboarding for a permission) never reaches the model step.
  const handleModelSelected = () => {
    setOnboardingStep("languages");
  };

  const handleLanguagesDone = () => {
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
          <Switch fallback={<Onboarding onModelSelected={NOOP} preview />}>
            <Match when={onboardingPreview() === "accessibility"}>
              <AccessibilityOnboarding onComplete={NOOP} preview />
            </Match>
            <Match when={onboardingPreview() === "languages"}>
              <LanguagesOnboarding onComplete={NOOP} preview />
            </Match>
          </Switch>
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
        <Match when={onboardingStep() === "languages"}>
          <LanguagesOnboarding onComplete={handleLanguagesDone} />
        </Match>
        <Match when={onboardingStep() === "done"}>
          <Show
            when={useNavigationStore().liveCanvas}
            fallback={
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
                    <div
                      ref={(el) => (settingsScrollRef = el)}
                      class="flex-1 overflow-y-auto"
                    >
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
            }
          >
            <LiveCanvas onExit={() => setLiveCanvas(false)} />
          </Show>
        </Match>
      </Switch>
    </>
  );
}

export default App;
