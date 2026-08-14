/**
 * Global keyboard shortcut definitions and metadata for S2B2S.
 */

export interface KeyboardShortcut {
  key: string;
  label: string;
  description: string;
  category: "Voice & AI" | "Navigation" | "General";
}

export const APP_SHORTCUTS: readonly KeyboardShortcut[] = [
  {
    key: "transcribe",
    label: "Push-to-Talk / Dictation",
    description:
      "Record microphone speech and transcribe into the active focused application",
    category: "Voice & AI",
  },
  {
    key: "converse",
    label: "Conversation Mode",
    description:
      "Start hands-free voice dialogue with Brain (LLM) and hear spoken responses",
    category: "Voice & AI",
  },
  {
    key: "speak_selection",
    label: "Read Aloud / Speak Selection",
    description:
      "Synthesize and speak highlighted text using the active TTS engine",
    category: "Voice & AI",
  },
  {
    key: "cancel",
    label: "Esc",
    description:
      "Cancel the ongoing recording, transcription, or TTS speech playback",
    category: "Voice & AI",
  },
  {
    key: "nav_general",
    label: "Ctrl + 1",
    description: "Navigate to General Settings",
    category: "Navigation",
  },
  {
    key: "nav_history",
    label: "Ctrl + 2",
    description: "Navigate to Transcription History",
    category: "Navigation",
  },
  {
    key: "nav_models",
    label: "Ctrl + 3",
    description: "Navigate to STT Models",
    category: "Navigation",
  },
  {
    key: "nav_speech",
    label: "Ctrl + 4",
    description: "Navigate to Speech & TTS Settings",
    category: "Navigation",
  },
  {
    key: "nav_brain",
    label: "Ctrl + 5",
    description: "Navigate to Brain & LLM Settings",
    category: "Navigation",
  },
  {
    key: "shortcuts_help",
    label: "?  or  Ctrl + /",
    description: "Toggle this Keyboard Shortcuts cheat sheet dialog",
    category: "General",
  },
  {
    key: "close_modal",
    label: "Esc",
    description: "Close active modal, popup, or overlay window",
    category: "General",
  },
] as const;
