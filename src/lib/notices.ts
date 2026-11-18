import type { TFunction } from "i18next";
import { platform } from "@tauri-apps/plugin-os";
import type { NoticeCode, NoticeSource } from "@/bindings";
import type { SidebarSection } from "@/components/Sidebar";

interface NoticeDisplay {
  /** An `errors.*Title` key; `build.rs` copies the same string to the tray. */
  title: string;
  /** What to do about it, when the title alone does not say. */
  explanation?: (t: TFunction) => string;
}

// One entry per code in `src-tauri/src/notices.rs` (see its header for how to
// add one).
const NOTICE_DISPLAY: Record<NoticeCode, NoticeDisplay> = {
  TranscriptionFailed: { title: "errors.transcriptionFailedTitle" },
  PasteFailed: {
    title: "errors.pasteFailedTitle",
    explanation: (t) => t("errors.pasteFailed"),
  },
  PasteBlockedElevated: {
    title: "errors.pasteBlockedElevatedTitle",
    explanation: (t) => t("errors.pasteBlockedElevated"),
  },
  NoMicrophone: {
    title: "errors.noInputDeviceTitle",
    explanation: (t) => t("errors.noInputDevice"),
  },
  MicPermissionDenied: {
    title: "errors.micPermissionDeniedTitle",
    explanation: (t) =>
      t(`errors.micPermissionDenied.${platform()}`, {
        defaultValue: t("errors.micPermissionDenied.generic"),
      }),
  },
  RecordingFailed: { title: "errors.recordingFailedTitle" },
  MicrophoneSilent: {
    title: "errors.microphoneSilentTitle",
    explanation: (t) => t("errors.microphoneSilent"),
  },
  NoModelSelected: {
    title: "errors.noModelSelectedTitle",
    explanation: (t) => t("errors.noModelSelected"),
  },
  ModelLoadFailed: { title: "errors.modelLoadFailedTitle" },
  PostProcessFailed: {
    title: "errors.postProcessFailedTitle",
    explanation: (t) => t("errors.postProcessFailed"),
  },
  MergeFailed: {
    title: "errors.mergeFailedTitle",
    explanation: (t) => t("errors.mergeFailed"),
  },
  FileTranscriptionFailed: { title: "errors.fileTranscriptionFailedTitle" },
  GpuFallback: {
    title: "errors.gpuFallbackTitle",
    explanation: (t) => t("errors.gpuFallback"),
  },
  LlamaServerFailed: {
    title: "errors.llamaServerFailedTitle",
    explanation: (t) => t("errors.llamaServerFailed"),
  },
};

export const noticeDisplay = (code: NoticeCode) => NOTICE_DISPLAY[code];

/** The page where the user can look into a source's notice. */
const NOTICE_PAGE: Record<NoticeSource, SidebarSection> = {
  dictation: "history",
  file: "fileTranscription",
  model: "models",
  gpu: "models",
  local_llm: "llama",
};

/** The page "Show" opens: the source's, unless the code names a better one. */
export const noticePage = (notice: {
  code: NoticeCode;
  source: NoticeSource;
}) =>
  notice.code === "NoModelSelected"
    ? "models"
    : notice.code === "MicrophoneSilent"
      ? "general"
      : NOTICE_PAGE[notice.source];
