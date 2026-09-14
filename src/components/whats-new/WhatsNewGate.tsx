import { createSignal, createEffect, Show } from "solid-js";

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

  // The compute phase is the dependency list: reading the settings there is
  // what makes the gate re-evaluate once they load (they are null at mount)
  // and on every later settings change.
  createEffect(
    () => ({ settings: settings(), loading: isLoading() }) as const,
    ({ settings: currentSettings, loading }) => {
      if (
        loading ||
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

  // Read inside the Show binding, not the body: a body-level `note()` would
  // snapshot the initial null and the modal could never open.
  return (
    <Show when={note()}>
      <WhatsNewModal note={note()!} open={isOpen()} onDismiss={dismiss} />
    </Show>
  );
};
