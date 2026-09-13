import { createSignal, createEffect } from "solid-js";

import { getVersion } from "@tauri-apps/api/app";
import { useSettings } from "../../hooks/useSettings";
import { findReleaseNoteToShow } from "./releaseNotes";
import type { ReleaseNote } from "./releaseNotes";
import { WhatsNewModal } from "./WhatsNewModal";

export const WhatsNewGate = () => {
  const { settings, isLoading, updateSetting } = useSettings();
  const [note, setNote] = createSignal<ReleaseNote | null>(null);
  const [isOpen, setIsOpen] = createSignal(false);
  let dismissedVersionRef: string | null = null;

  createEffect(
    () => undefined,
    () => {
      const currentSettings = settings();
      if (
        isLoading() ||
        !currentSettings ||
        !currentSettings.show_whats_new_on_update
      ) {
        setIsOpen(false);
        setNote(null);
        return;
      }

      let cancelled = false;

      const loadReleaseNote = async () => {
        try {
          const currentVersion = await getVersion();
          if (cancelled) return;

          const releaseNote = findReleaseNoteToShow({
            currentVersion,
            lastSeenVersion: currentSettings.whats_new_last_seen_version ?? "",
          });

          if (!releaseNote || dismissedVersionRef === releaseNote.version) {
            setIsOpen(false);
            setNote(null);
            return;
          }

          setNote(releaseNote);
          setIsOpen(true);
        } catch (error) {
          console.error("Failed to load release notes:", error);
        }
      };

      void loadReleaseNote();

      return () => {
        cancelled = true;
      };
    },
  );

  const dismiss = () => {
    const current = note();
    if (!current) return;

    dismissedVersionRef = current.version;
    setIsOpen(false);
    void updateSetting("whats_new_last_seen_version", current.version);
  };

  const current = note();
  if (!current) return null;

  return <WhatsNewModal note={current} open={isOpen()} onDismiss={dismiss} />;
};
