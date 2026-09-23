import { createSignal, createEffect, Match, Switch } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import {
  getKeyName,
  formatKeyCombination,
  MODIFIERS,
  normalizeKey,
} from "../../lib/utils/keyboard";
import { ResetButton } from "../ui/ResetButton";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import { logCommandResult } from "./logCommandResult";
import { sessionToast as toast } from "@/lib/sessionToast";

interface GlobalShortcutInputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  shortcutId: string;
  disabled?: boolean;
}

export const GlobalShortcutInput = (props: GlobalShortcutInputProps) => {
  const { t } = useTranslation();
  const { getSetting, updateBinding, resetBinding, isUpdating, isLoading } =
    useSettings();
  const [keyPressed, setKeyPressed] = createSignal<string[]>([]);
  const [recordedKeys, setRecordedKeys] = createSignal<string[]>([]);
  const [editingShortcutId, setEditingShortcutId] = createSignal<string | null>(
    null,
  );
  const [originalBinding, setOriginalBinding] = createSignal<string>("");
  const shortcutRefs = new Map<string, HTMLDivElement | null>();
  const osType = useOsType();

  const descriptionMode = () => props.descriptionMode ?? "tooltip";
  const grouped = () => props.grouped ?? false;
  const bindings = () => getSetting("bindings") || {};
  const binding = () => bindings()[props.shortcutId];

  const startRecording = async (id: string) => {
    if (editingShortcutId() === id) return;

    await logCommandResult(
      "Failed to suspend bindings",
      commands.suspendAllBindings(),
    );

    setOriginalBinding(bindings()[id]?.current_binding || "");
    setEditingShortcutId(id);
    setKeyPressed([]);
    setRecordedKeys([]);
  };

  const formatCurrentKeys = (): string => {
    if (recordedKeys().length === 0)
      return t("settings.general.shortcut.pressKeys");

    return formatKeyCombination(recordedKeys().join("+"), osType);
  };

  const setShortcutRef = (id: string, ref: HTMLDivElement | null) => {
    shortcutRefs.set(id, ref);
  };

  createEffect(
    () => editingShortcutId(),
    (id) => {
      if (id === null) return;

      let cleanup = false;

      const handleKeyDown = async (e: KeyboardEvent) => {
        if (cleanup) return;
        if (e.repeat) return;
        e.preventDefault();

        const rawKey = getKeyName(e, osType);
        const key = normalizeKey(rawKey);

        if (!keyPressed().includes(key)) {
          setKeyPressed((prev) => [...prev, key]);
          if (!recordedKeys().includes(key)) {
            setRecordedKeys((prev) => [...prev, key]);
          }
        }
      };

      const handleKeyUp = async (e: KeyboardEvent) => {
        if (cleanup) return;
        e.preventDefault();

        const rawKey = getKeyName(e, osType);
        const key = normalizeKey(rawKey);

        setKeyPressed((prev) => prev.filter((k) => k !== key));

        const updatedKeyPressed = keyPressed().filter((k) => k !== key);
        if (updatedKeyPressed.length === 0 && recordedKeys().length > 0) {
          const sortedKeys = recordedKeys().toSorted((a, b) => {
            const aIsModifier = MODIFIERS.has(a.toLowerCase());
            const bIsModifier = MODIFIERS.has(b.toLowerCase());
            if (aIsModifier && !bIsModifier) return -1;
            if (!aIsModifier && bIsModifier) return 1;
            return 0;
          });
          const newShortcut = sortedKeys.join("+");

          if (bindings()[id]) {
            try {
              await updateBinding(id, newShortcut);
            } catch (error) {
              console.error("Failed to change binding:", error);
              toast.error(
                t("settings.general.shortcut.errors.set", {
                  error: String(error),
                }),
              );
              if (originalBinding()) {
                try {
                  await updateBinding(id, originalBinding());
                } catch (resetError) {
                  console.error("Failed to reset binding:", resetError);
                  toast.error(t("settings.general.shortcut.errors.reset"));
                }
              }
            }

            await logCommandResult(
              "Failed to resume bindings",
              commands.resumeAllBindings(),
            );

            setEditingShortcutId(null);
            setKeyPressed([]);
            setRecordedKeys([]);
            setOriginalBinding("");
          }
        }
      };

      const handleClickOutside = async (e: MouseEvent) => {
        if (cleanup) return;
        const activeElement = shortcutRefs.get(id);
        if (activeElement && !activeElement.contains(e.target as Node)) {
          if (originalBinding()) {
            try {
              await updateBinding(id, originalBinding());
            } catch (error) {
              console.error("Failed to restore original binding:", error);
              toast.error(t("settings.general.shortcut.errors.restore"));
            }
          }
          await logCommandResult(
            "Failed to resume bindings",
            commands.resumeAllBindings(),
          );
          setEditingShortcutId(null);
          setKeyPressed([]);
          setRecordedKeys([]);
          setOriginalBinding("");
        }
      };

      window.addEventListener("keydown", handleKeyDown);
      window.addEventListener("keyup", handleKeyUp);
      window.addEventListener("click", handleClickOutside);

      return () => {
        cleanup = true;
        window.removeEventListener("keydown", handleKeyDown);
        window.removeEventListener("keyup", handleKeyUp);
        window.removeEventListener("click", handleClickOutside);
      };
    },
  );

  // One placeholder for the three "nothing to edit" states. The body runs
  // once, so each state is a `Match` rather than an early return: settings
  // load after mount and bindings change under the component.
  const placeholder = (description: string) => (
    <SettingContainer
      title={t("settings.general.shortcut.title")}
      description={description}
      descriptionMode={descriptionMode()}
      grouped={grouped()}
    >
      <div class="text-sm text-mid-gray">
        {isLoading()
          ? t("settings.general.shortcut.loading")
          : t("settings.general.shortcut.none")}
      </div>
    </SettingContainer>
  );

  return (
    <Switch>
      <Match when={isLoading() || Object.keys(bindings()).length === 0}>
        {placeholder(t("settings.general.shortcut.description"))}
      </Match>
      <Match when={!binding()}>
        {placeholder(t("settings.general.shortcut.notFound"))}
      </Match>
      <Match when={binding()}>
        {(b) => (
          <SettingContainer
            title={t(
              `settings.general.shortcut.bindings.${props.shortcutId}.name`,
              b().name,
            )}
            description={t(
              `settings.general.shortcut.bindings.${props.shortcutId}.description`,
              b().description,
            )}
            descriptionMode={descriptionMode()}
            grouped={grouped()}
            disabled={props.disabled ?? false}
            layout="horizontal"
          >
            <div class="flex items-center space-x-1">
              {editingShortcutId() === props.shortcutId ? (
                <div
                  ref={(ref) => setShortcutRef(props.shortcutId, ref)}
                  class="px-2 py-1 text-sm font-semibold border border-accent bg-accent/30 rounded-md"
                >
                  {formatCurrentKeys()}
                </div>
              ) : (
                <button
                  type="button"
                  class="px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-accent/10 rounded-md cursor-pointer hover:border-accent text-start"
                  onClick={() => startRecording(props.shortcutId)}
                >
                  {formatKeyCombination(b().current_binding, osType)}
                </button>
              )}
              <ResetButton
                onClick={() => resetBinding(props.shortcutId)}
                disabled={isUpdating(`binding_${props.shortcutId}`)}
              />
            </div>
          </SettingContainer>
        )}
      </Match>
    </Switch>
  );
};
