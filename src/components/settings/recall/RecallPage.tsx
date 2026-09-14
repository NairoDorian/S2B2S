import { createSignal, createEffect, createMemo, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "@/i18n/useTranslation";
import { sessionToast as toast } from "@/lib/sessionToast";
import { useSettings } from "@/hooks/useSettings";
import {
  FilePlus,
  FileText,
  FolderOpen,
  Mic,
  Square,
  Loader2,
} from "@/components/icons/lucide";
import { Button } from "@/components/ui/Button";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { MarkdownContent } from "@/components/whats-new/MarkdownContent";
import { useRecallStore } from "@/stores/recallStore";
import { commands, events, type RecallNoteMeta } from "@/bindings";

const formatDate = (ms: number) =>
  new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

const inputClass =
  "w-full rounded-lg border border-mid-gray/20 bg-background px-3 py-1.5 text-sm focus:outline-none focus:border-accent/50";

export const RecallPage = () => {
  const { t } = useTranslation();
  const store = useRecallStore();
  const { getSetting, updateSetting } = useSettings();
  const [mode, setMode] = createSignal<"write" | "preview">("write");
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  let bodyRef: HTMLTextAreaElement | undefined;

  // Encryption flows: the enable form, the unlock field, the disable flow.
  const [encFormOpen, setEncFormOpen] = createSignal(false);
  const [encPass, setEncPass] = createSignal("");
  const [encPass2, setEncPass2] = createSignal("");
  const [encBusy, setEncBusy] = createSignal(false);
  const [unlockPass, setUnlockPass] = createSignal("");
  const [disableOpen, setDisableOpen] = createSignal(false);
  const [disablePass, setDisablePass] = createSignal("");

  createEffect(
    () => undefined,
    () => {
      void store.initialize();
    },
  );

  // A different note opened (or the editor closed): the two-step delete
  // must never carry over to whatever is on screen next.
  createEffect(
    () => store.activeId,
    () => {
      setConfirmDelete(false);
    },
  );

  const dirty = createMemo(() => {
    const { draft, savedDraft } = store;
    return (
      draft.title !== savedDraft.title ||
      draft.tags !== savedDraft.tags ||
      draft.body !== savedDraft.body
    );
  });

  const wordCount = createMemo(
    () => store.draft.body.split(/\s+/).filter(Boolean).length,
  );

  const locked = createMemo(
    () =>
      store.encryption?.enabled === true && store.encryption.unlocked === false,
  );

  const handleNew = async () => {
    const error = await store.createNote(t("settings.recall.untitled"));
    if (error) toast.error(error);
    setMode("write");
  };

  const handleSave = async () => {
    const error = await store.save();
    if (error) toast.error(error);
  };

  const handleDelete = async () => {
    const id = store.activeId;
    if (!id) return;
    if (!confirmDelete()) {
      setConfirmDelete(true);
      return;
    }
    const error = await store.remove(id);
    if (error) toast.error(error);
  };

  /**
   * Insert dictated text exactly where the caret is — the whole point of
   * dictate-to-cursor. In preview mode there is no caret, so the editor
   * switches back to write mode first.
   */
  const insertAtCaret = (text: string) => {
    if (mode() === "preview") setMode("write");
    const el = bodyRef;
    const body = store.draft.body;
    if (!el) {
      store.setDraft({ body: body + text });
      return;
    }
    const start = el.selectionStart ?? body.length;
    const end = el.selectionEnd ?? start;
    store.setDraft({ body: body.slice(0, start) + text + body.slice(end) });
    const caret = start + text.length;
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(caret, caret);
    });
  };

  const handleDictate = async () => {
    if (store.dictate === "recording") {
      const result = await store.dictateStop();
      if ("error" in result) {
        toast.error(result.error);
        return;
      }
      if (result.text) insertAtCaret(result.text);
      return;
    }
    if (store.dictate === "transcribing") return;
    const error = await store.dictateStart();
    if (error) toast.error(error);
  };

  // While the editor's caret is live, the transcription and Multi-STT
  // hotkeys deliver their text here instead of pasting + writing History:
  // the backend routes the take to the vault, and it arrives as this event.
  createEffect(
    () => undefined,
    () => {
      const unlisten = events.recallInsertTextEvent.listen((e) => {
        if (e.payload.text) insertAtCaret(e.payload.text);
      });
      return () => {
        void unlisten.then((fn) => fn());
        void commands.recallSetInsertionMode(false);
      };
    },
  );

  // Editor closed or switched to preview: there is no caret to arm.
  createEffect(
    () => [store.activeId, mode()] as const,
    () => {
      if (!store.activeId || mode() !== "write") {
        void commands.recallSetInsertionMode(false);
      }
    },
  );

  const handleEnableEncryption = async () => {
    if (encPass().length < 8) {
      toast.error(t("settings.recall.enc.tooShort"));
      return;
    }
    if (encPass() !== encPass2()) {
      toast.error(t("settings.recall.enc.mismatch"));
      return;
    }
    setEncBusy(true);
    const result = await store.enableEncryption(encPass());
    setEncBusy(false);
    if ("error" in result) {
      toast.error(result.error);
      return;
    }
    setEncFormOpen(false);
    setEncPass("");
    setEncPass2("");
    toast.success(
      t("settings.recall.enc.encrypted", {
        count: result.notesEncrypted,
        path: result.backupDir,
      }),
    );
  };

  const handleUnlock = async () => {
    const error = await store.unlock(unlockPass());
    if (error) {
      toast.error(error);
      return;
    }
    setUnlockPass("");
  };

  const handleLock = async () => {
    const error = await store.lock();
    if (error) toast.error(error);
  };

  // The vault is a folder the user owns: point the app at an existing
  // vault (a backup, a synced folder) or create a new one anywhere. The
  // backend locks the session on every change, so an encrypted vault is
  // unlocked explicitly afterwards.
  const customVaultDir = () => getSetting("recall")?.output_dir ?? null;

  const pickVaultFolder = async () => {
    const selection = await open({ directory: true, multiple: false });
    if (!selection || Array.isArray(selection)) return;
    await updateSetting("recall", {
      ...getSetting("recall"),
      output_dir: selection,
    });
    store.closeNote();
    await store.refreshEncryption();
  };

  const useDefaultVaultFolder = async () => {
    await updateSetting("recall", {
      ...getSetting("recall"),
      output_dir: null,
    });
    store.closeNote();
    await store.refreshEncryption();
  };

  const handleDisableEncryption = async () => {
    const error = await store.disableEncryption(disablePass());
    if (error) {
      toast.error(error);
      return;
    }
    setDisableOpen(false);
    setDisablePass("");
    toast.success(t("settings.recall.enc.decrypted"));
  };

  const noteLabel = (note: RecallNoteMeta) => {
    const tags = note.tags.join(", ");
    return tags
      ? `${formatDate(note.updated_ms)} · ${tags}`
      : formatDate(note.updated_ms);
  };

  return (
    <div class="max-w-5xl w-full mx-auto space-y-6 pb-8">
      <SettingsGroup
        title={t("settings.recall.title")}
        description={t("settings.recall.description")}
      >
        <div class="p-3 space-y-3">
          <Show
            when={!locked()}
            fallback={
              <div class="flex flex-col items-center justify-center gap-2 py-10 rounded-lg border border-dashed border-mid-gray/20 text-center">
                <p class="text-sm font-medium">
                  {t("settings.recall.enc.lockedTitle")}
                </p>
                <p class="text-xs text-mid-gray max-w-sm">
                  {t("settings.recall.enc.lockedHint")}
                </p>
              </div>
            }
          >
            <div class="flex items-center justify-between gap-3">
              <Show
                when={store.vault}
                fallback={<span class="text-sm text-mid-gray" />}
              >
                <span class="text-xs text-mid-gray">
                  {t("settings.recall.vaultStats", {
                    notes: store.vault!.note_count,
                    audio: store.vault!.audio_count,
                  })}
                </span>
              </Show>
              <div class="flex items-center gap-2">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void store.openVaultFolder()}
                >
                  <span class="flex items-center gap-1.5">
                    <FolderOpen class="w-4 h-4" />
                    {t("settings.recall.openFolder")}
                  </span>
                </Button>
                <Button size="sm" onClick={handleNew}>
                  <span class="flex items-center gap-1.5">
                    <FilePlus class="w-4 h-4" />
                    {t("settings.recall.newNote")}
                  </span>
                </Button>
              </div>
            </div>

            <Show
              when={store.notes.length > 0}
              fallback={
                <div class="flex flex-col items-center justify-center gap-2 py-10 rounded-lg border border-dashed border-mid-gray/20 text-center">
                  <FileText class="w-8 h-8 text-mid-gray" />
                  <p class="text-sm font-medium">
                    {t("settings.recall.emptyTitle")}
                  </p>
                  <p class="text-xs text-mid-gray max-w-sm">
                    {t("settings.recall.emptyHint")}
                  </p>
                </div>
              }
            >
              <div class="flex gap-3 items-start">
                {/* Entries */}
                <ul class="w-60 shrink-0 space-y-1 max-h-[28rem] overflow-y-auto pr-1">
                  <For each={store.notes}>
                    {(note) => (
                      <li>
                        <button
                          type="button"
                          class={`w-full text-left px-3 py-2 rounded-lg border transition-colors cursor-pointer ${
                            store.activeId === note.id
                              ? "border-accent/50 bg-accent/10"
                              : "border-transparent hover:bg-mid-gray/10"
                          }`}
                          onClick={() => void store.openNote(note.id)}
                        >
                          <span class="block text-sm font-medium truncate">
                            {note.title}
                          </span>
                          <span class="block text-xs text-mid-gray truncate">
                            {noteLabel(note)}
                          </span>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>

                {/* Viewer / editor */}
                <Show when={store.activeId} fallback={<div class="flex-1" />}>
                  <div class="flex-1 space-y-2 rounded-lg border border-mid-gray/20 bg-background p-3">
                    <div class="flex gap-2">
                      <input
                        type="text"
                        class="flex-1 min-w-0 rounded-lg border border-mid-gray/20 bg-background px-3 py-1.5 text-sm font-medium focus:outline-none focus:border-accent/50"
                        placeholder={t("settings.recall.titlePlaceholder")}
                        value={store.draft.title}
                        onInput={(e) =>
                          store.setDraft({ title: e.currentTarget.value })
                        }
                      />
                      <div class="flex rounded-lg border border-mid-gray/20 overflow-hidden">
                        <button
                          type="button"
                          class={`px-3 text-xs cursor-pointer ${mode() === "write" ? "bg-accent/20" : "hover:bg-mid-gray/10"}`}
                          onClick={() => setMode("write")}
                        >
                          {t("settings.recall.edit")}
                        </button>
                        <button
                          type="button"
                          class={`px-3 text-xs cursor-pointer ${mode() === "preview" ? "bg-accent/20" : "hover:bg-mid-gray/10"}`}
                          onClick={() => setMode("preview")}
                        >
                          {t("settings.recall.preview")}
                        </button>
                      </div>
                    </div>

                    <input
                      type="text"
                      class="w-full rounded-lg border border-mid-gray/20 bg-background px-3 py-1.5 text-xs focus:outline-none focus:border-accent/50"
                      placeholder={t("settings.recall.tagsPlaceholder")}
                      value={store.draft.tags}
                      onInput={(e) =>
                        store.setDraft({ tags: e.currentTarget.value })
                      }
                    />

                    <Show
                      when={mode() === "write"}
                      fallback={
                        <div class="h-80 overflow-y-auto rounded-lg border border-mid-gray/20 bg-background px-3 py-2">
                          <MarkdownContent markdown={store.draft.body} />
                        </div>
                      }
                    >
                      <textarea
                        ref={(el) => {
                          bodyRef = el;
                        }}
                        onFocus={() =>
                          void commands.recallSetInsertionMode(true)
                        }
                        onBlur={() =>
                          void commands.recallSetInsertionMode(false)
                        }
                        class="w-full h-80 resize-y rounded-lg border border-mid-gray/20 bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:border-accent/50"
                        placeholder={t("settings.recall.bodyPlaceholder")}
                        value={store.draft.body}
                        onInput={(e) =>
                          store.setDraft({ body: e.currentTarget.value })
                        }
                      />
                    </Show>

                    <div class="flex items-center justify-between gap-2">
                      <span class="text-xs text-mid-gray">
                        {t("settings.recall.words", { count: wordCount() })}
                      </span>
                      <div class="flex items-center gap-2">
                        <Show
                          when={store.dictate === "idle"}
                          fallback={
                            <Button
                              variant="danger-ghost"
                              size="sm"
                              onClick={handleDictate}
                            >
                              <span class="flex items-center gap-1.5">
                                {store.dictate === "recording" ? (
                                  <Square class="w-4 h-4 text-red-500" />
                                ) : (
                                  <Loader2 class="w-4 h-4 animate-spin" />
                                )}
                                {store.dictate === "recording"
                                  ? t("settings.recall.stopDictation")
                                  : t("settings.recall.transcribing")}
                              </span>
                            </Button>
                          }
                        >
                          <Button
                            variant="secondary"
                            size="sm"
                            onClick={handleDictate}
                          >
                            <span class="flex items-center gap-1.5">
                              <Mic class="w-4 h-4" />
                              {t("settings.recall.dictate")}
                            </span>
                          </Button>
                        </Show>
                        <Show
                          when={!confirmDelete()}
                          fallback={
                            <Button
                              variant="danger"
                              size="sm"
                              onClick={handleDelete}
                            >
                              {t("settings.recall.confirmDelete")}
                            </Button>
                          }
                        >
                          <Button
                            variant="danger-ghost"
                            size="sm"
                            onClick={handleDelete}
                          >
                            {t("settings.recall.delete")}
                          </Button>
                        </Show>
                        <Button
                          size="sm"
                          disabled={!dirty() || store.saving}
                          onClick={handleSave}
                        >
                          {t("settings.recall.save")}
                        </Button>
                      </div>
                    </div>
                  </div>
                </Show>
              </div>
            </Show>
          </Show>
        </div>
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.recall.enc.title")}
        description={t("settings.recall.enc.description")}
      >
        <div class="p-3 space-y-3">
          {/* Vault location: pick any folder, existing vault or new one */}
          <div class="flex items-center justify-between gap-3">
            <span class="text-xs text-mid-gray min-w-0 truncate">
              {t("settings.recall.enc.location")}: {store.vault?.root ?? ""}
            </span>
            <div class="flex items-center gap-2 shrink-0">
              <Show when={customVaultDir()}>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={useDefaultVaultFolder}
                >
                  {t("settings.recall.enc.useDefault")}
                </Button>
              </Show>
              <Button variant="secondary" size="sm" onClick={pickVaultFolder}>
                {t("settings.recall.enc.choose")}
              </Button>
            </div>
          </div>

          {/* Off: the enable flow */}
          <Show
            when={store.encryption?.enabled}
            fallback={
              <Show
                when={encFormOpen()}
                fallback={
                  <div class="flex items-center justify-between gap-3">
                    <span class="text-xs text-mid-gray">
                      {t("settings.recall.enc.off")}
                    </span>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => setEncFormOpen(true)}
                    >
                      {t("settings.recall.enc.enable")}
                    </Button>
                  </div>
                }
              >
                <div class="space-y-2 rounded-lg border border-mid-gray/20 bg-background p-3">
                  <input
                    type="password"
                    class={inputClass}
                    placeholder={t("settings.recall.enc.passphrase")}
                    value={encPass()}
                    onInput={(e) => setEncPass(e.currentTarget.value)}
                  />
                  <input
                    type="password"
                    class={inputClass}
                    placeholder={t("settings.recall.enc.confirmPassphrase")}
                    value={encPass2()}
                    onInput={(e) => setEncPass2(e.currentTarget.value)}
                  />
                  <p class="text-xs text-mid-gray">
                    {t("settings.recall.enc.passphraseHint")}
                  </p>
                  <div class="flex items-center gap-2 justify-end">
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={encBusy()}
                      onClick={() => setEncFormOpen(false)}
                    >
                      {t("common.cancel")}
                    </Button>
                    <Button
                      size="sm"
                      disabled={encBusy()}
                      onClick={handleEnableEncryption}
                    >
                      <span class="flex items-center gap-1.5">
                        <Show when={encBusy()}>
                          <Loader2 class="w-4 h-4 animate-spin" />
                        </Show>
                        {encBusy()
                          ? t("settings.recall.enc.encrypting")
                          : t("settings.recall.enc.enable")}
                      </span>
                    </Button>
                  </div>
                </div>
              </Show>
            }
          >
            {/* On: status + lock / disable */}
            <div class="space-y-3">
              <div class="flex items-center justify-between gap-3">
                <span class="text-xs text-mid-gray">
                  {t("settings.recall.enc.on")}
                </span>
                <div class="flex items-center gap-2">
                  <Show
                    when={!locked()}
                    fallback={
                      <span class="text-xs text-mid-gray">
                        {t("settings.recall.enc.lockedBadge")}
                      </span>
                    }
                  >
                    <Button variant="secondary" size="sm" onClick={handleLock}>
                      {t("settings.recall.enc.lock")}
                    </Button>
                  </Show>
                  <Button
                    variant="danger-ghost"
                    size="sm"
                    onClick={() => setDisableOpen(!disableOpen())}
                  >
                    {t("settings.recall.enc.disable")}
                  </Button>
                </div>
              </div>

              {/* Locked: the unlock field also lives here for reachability */}
              <Show when={locked()}>
                <div class="space-y-2 rounded-lg border border-mid-gray/20 bg-background p-3">
                  <input
                    type="password"
                    class={inputClass}
                    placeholder={t("settings.recall.enc.passphrase")}
                    value={unlockPass()}
                    onInput={(e) => setUnlockPass(e.currentTarget.value)}
                  />
                  <div class="flex justify-end">
                    <Button size="sm" onClick={handleUnlock}>
                      {t("settings.recall.enc.unlock")}
                    </Button>
                  </div>
                </div>
              </Show>

              {/* Disable: passphrase + confirm, the reverse of enable */}
              <Show when={disableOpen()}>
                <div class="space-y-2 rounded-lg border border-red-500/30 bg-red-500/5 p-3">
                  <input
                    type="password"
                    class={inputClass}
                    placeholder={t("settings.recall.enc.passphrase")}
                    value={disablePass()}
                    onInput={(e) => setDisablePass(e.currentTarget.value)}
                  />
                  <p class="text-xs text-mid-gray">
                    {t("settings.recall.enc.disableHint")}
                  </p>
                  <div class="flex items-center gap-2 justify-end">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setDisableOpen(false)}
                    >
                      {t("common.cancel")}
                    </Button>
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={handleDisableEncryption}
                    >
                      {t("settings.recall.enc.disableConfirm")}
                    </Button>
                  </div>
                </div>
              </Show>
            </div>
          </Show>
        </div>
      </SettingsGroup>
    </div>
  );
};
