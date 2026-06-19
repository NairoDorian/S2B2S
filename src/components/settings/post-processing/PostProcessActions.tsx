import React, { useMemo, useState } from "react";
import { ChevronRight, Plus, Sparkles, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type PostProcessAction } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import {
  ACTION_ICON_NAMES,
  DEFAULT_ACTION_ICON,
  getActionIcon,
} from "@/lib/constants/actionIcons";
import {
  Dialog,
  Dropdown,
  SettingsGroup,
  Textarea,
} from "@/components/ui";
import { Input } from "../../ui/Input";
import { Button } from "../../ui/Button";
import { ShortcutInput } from "../ShortcutInput";

const Kbd: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <span className="inline-flex items-center px-1.5 h-5 rounded-md border border-mid-gray/30 bg-mid-gray/10 text-[11px] font-semibold text-text/70 leading-none">
    {children}
  </span>
);

interface IconPickerProps {
  value: string;
  onChange: (icon: string) => void;
}

const IconPicker: React.FC<IconPickerProps> = ({ value, onChange }) => (
  <div className="grid grid-cols-10 gap-1.5">
    {ACTION_ICON_NAMES.map((name) => {
      const Icon = getActionIcon(name);
      const isActive = name === value;
      return (
        <button
          key={name}
          type="button"
          onClick={() => onChange(name)}
          className={`flex items-center justify-center aspect-square rounded-lg border transition-colors ${
            isActive
              ? "border-logo-primary bg-logo-primary/20 text-logo-primary"
              : "border-mid-gray/20 hover:border-logo-primary/50 hover:bg-mid-gray/10 text-text/60"
          }`}
        >
          <Icon className="w-4 h-4" />
        </button>
      );
    })}
  </div>
);

interface ActionDialogProps {
  open: boolean;
  action: PostProcessAction | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}

const ActionDialog: React.FC<ActionDialogProps> = ({
  open,
  action,
  onClose,
  onSaved,
}) => {
  const { t } = useTranslation();
  const { settings, refreshSettings } = useSettings();

  const savedModels = settings?.llm_models || [];
  const actions = settings?.post_process_actions || [];

  const [name, setName] = useState(action?.name ?? "");
  const [prompt, setPrompt] = useState(action?.prompt ?? "");
  const [icon, setIcon] = useState(action?.icon ?? DEFAULT_ACTION_ICON);
  const [llmModelId, setLlmModelId] = useState<string | null>(
    action?.llm_model_id ?? savedModels[0]?.id ?? null,
  );
  const [triggerKey, setTriggerKey] = useState<number | null>(
    action?.trigger_key ?? null,
  );
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const modelOptions = useMemo(
    () => savedModels.map((m) => ({ value: m.id, label: m.label })),
    [savedModels],
  );

  const triggerKeyOptions = useMemo(() => {
    const usedKeys = new Set(
      actions
        .filter((a) => a.id !== action?.id)
        .map((a) => a.trigger_key)
        .filter((k): k is number => k != null),
    );
    const options: { value: string; label: string; disabled?: boolean }[] = [
      { value: "none", label: t("settings.postProcessing.actions.noKey") },
    ];
    for (let k = 1; k <= 9; k++) {
      options.push({
        value: String(k),
        label: usedKeys.has(k)
          ? t("settings.postProcessing.actions.keyTaken", { key: k })
          : String(k),
        disabled: usedKeys.has(k),
      });
    }
    return options;
  }, [actions, action?.id, t]);

  const canSave = name.trim().length > 0 && prompt.trim().length > 0;

  const handleSave = React.useCallback(async () => {
    if (!canSave) return;
    setIsSaving(true);
    setError(null);
    try {
      if (action) {
        const result = await commands.updatePostProcessAction(
          action.id,
          name.trim(),
          prompt.trim(),
          llmModelId,
          icon,
          triggerKey,
        );
        if (result.status === "ok") {
          await refreshSettings();
          onSaved(action.id);
        } else {
          setError(String(result.error));
        }
      } else {
        const result = await commands.addPostProcessAction(
          name.trim(),
          prompt.trim(),
          llmModelId,
          icon,
          triggerKey,
        );
        if (result.status === "ok") {
          await refreshSettings();
          onSaved(result.data.id);
        } else {
          setError(String(result.error));
        }
      }
    } finally {
      setIsSaving(false);
    }
  }, [action, canSave, name, prompt, llmModelId, icon, triggerKey, refreshSettings, onSaved]);

  const handleDelete = React.useCallback(async () => {
    if (!action) return;
    const result = await commands.deletePostProcessAction(action.id);
    if (result.status === "ok") {
      await refreshSettings();
      onClose();
    }
  }, [action, refreshSettings, onClose]);

  const footer = (
    <>
      {action && (
        <Button
          variant="danger-ghost"
          size="md"
          onClick={handleDelete}
          className="mr-auto flex items-center gap-1.5"
        >
          <Trash2 className="w-4 h-4" />
          {t("common.delete")}
        </Button>
      )}
      <Button variant="secondary" size="md" onClick={onClose}>
        {t("common.cancel")}
      </Button>
      <Button
        variant="primary"
        size="md"
        onClick={handleSave}
        disabled={!canSave || isSaving}
      >
        {action ? t("common.save") : t("common.create")}
      </Button>
    </>
  );

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={
        action
          ? t("settings.postProcessing.actions.editTitle")
          : t("settings.postProcessing.actions.newTitle")
      }
      description={t("settings.postProcessing.actions.dialogSubtitle")}
      footer={footer}
    >
      <div className="space-y-5">
        <div className="space-y-1.5">
          <label className="text-sm font-medium text-text/80">
            {t("settings.postProcessing.actions.name")}
          </label>
          <Input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("settings.postProcessing.actions.namePlaceholder")}
            variant="compact"
            className="w-full"
          />
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium text-text/80">
            {t("settings.postProcessing.actions.icon")}
          </label>
          <IconPicker value={icon} onChange={setIcon} />
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium text-text/80">
            {t("settings.postProcessing.actions.prompt")}
          </label>
          <Textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder={t("settings.postProcessing.actions.promptPlaceholder")}
            className="w-full block min-h-[120px] font-normal"
          />
          <p className="text-xs text-text/45">
            {t("settings.postProcessing.actions.promptHint")}
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium text-text/80">
            {t("settings.postProcessing.actions.model")}
          </label>
          {modelOptions.length > 0 ? (
            <Dropdown
              selectedValue={llmModelId}
              options={modelOptions}
              onSelect={(value) => setLlmModelId(value)}
              placeholder={t("settings.postProcessing.actions.modelPlaceholder")}
              className="w-full"
            />
          ) : (
            <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2">
              <p className="text-xs text-amber-500">
                {t("settings.postProcessing.actions.noModels")}
              </p>
            </div>
          )}
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <label className="text-sm font-medium text-text/80">
              {t("settings.postProcessing.actions.triggerKey")}
            </label>
            <Dropdown
              selectedValue={triggerKey != null ? String(triggerKey) : "none"}
              options={triggerKeyOptions}
              onSelect={(value) =>
                setTriggerKey(value === "none" ? null : Number(value))
              }
              placeholder={t("settings.postProcessing.actions.noKey")}
              className="w-full"
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-sm font-medium text-text/80">
              {t("settings.postProcessing.actions.shortcut")}
            </label>
            {action ? (
              <ShortcutInput shortcutId={`ppa_${action.id}`} />
            ) : (
              <p className="text-xs text-text/45 pt-1.5">
                {t("settings.postProcessing.actions.shortcutAfterSave")}
              </p>
            )}
          </div>
        </div>

        {error && (
          <div className="rounded-lg border border-red-500/30 bg-red-500/5 px-3 py-2">
            <p className="text-xs text-red-500">{error}</p>
          </div>
        )}
      </div>
    </Dialog>
  );
};

interface ActionCardProps {
  action: PostProcessAction;
  modelLabel?: string;
  shortcut?: string;
  onClick: () => void;
}

const ActionCard: React.FC<ActionCardProps> = ({
  action,
  modelLabel,
  shortcut,
  onClick,
}) => {
  const { t } = useTranslation();
  const Icon = getActionIcon(action.icon);

  return (
    <button
      type="button"
      onClick={onClick}
      className="group w-full flex items-center gap-3.5 p-3 rounded-xl border border-mid-gray/15 bg-mid-gray/[0.03] hover:border-logo-primary/40 hover:bg-logo-primary/[0.05] transition-colors text-start"
    >
      <div className="flex items-center justify-center w-10 h-10 rounded-lg bg-logo-primary/15 text-logo-primary shrink-0">
        <Icon className="w-5 h-5" />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold truncate">{action.name}</p>
        <p className="text-xs text-text/50 truncate mt-0.5">
          {modelLabel ?? t("settings.postProcessing.actions.noModelShort")}
        </p>
      </div>
      <div className="flex items-center gap-1.5 shrink-0">
        {action.trigger_key != null && <Kbd>{action.trigger_key}</Kbd>}
        {shortcut && <Kbd>{shortcut}</Kbd>}
      </div>
      <ChevronRight className="w-4 h-4 text-text/25 group-hover:text-logo-primary shrink-0 transition-colors" />
    </button>
  );
};

export const PostProcessActions: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();

  const actions = settings?.post_process_actions || [];
  const models = settings?.llm_models || [];
  const bindings = settings?.bindings || {};

  const [editingId, setEditingId] = useState<string | null>(null);

  const editingAction =
    editingId && editingId !== "new"
      ? (actions.find((a) => a.id === editingId) ?? null)
      : null;
  const dialogOpen = editingId !== null;

  const modelLabel = (id: string | null | undefined) =>
    models.find((m) => m.id === id)?.label;

  const actionShortcut = (id: string) => {
    const raw = bindings[`ppa_${id}`]?.current_binding;
    return raw && raw.trim() ? formatKeyCombination(raw, osType) : undefined;
  };

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h3 className="text-base font-semibold">
            {t("settings.postProcessing.actions.title")}
          </h3>
          <p className="text-xs text-text/55 mt-1">
            {t("settings.postProcessing.actions.description")}
          </p>
        </div>
        <Button
          variant="primary"
          size="md"
          onClick={() => setEditingId("new")}
          className="flex items-center gap-1.5 shrink-0"
        >
          <Plus className="w-4 h-4" />
          {t("settings.postProcessing.actions.newAction")}
        </Button>
      </div>

      {actions.length === 0 ? (
        <div className="flex flex-col items-center justify-center text-center py-10 px-6 rounded-2xl border border-dashed border-mid-gray/25">
          <div className="w-12 h-12 rounded-xl bg-logo-primary/15 text-logo-primary flex items-center justify-center mb-3">
            <Sparkles className="w-6 h-6" />
          </div>
          <p className="text-sm font-medium">
            {t("settings.postProcessing.actions.emptyTitle")}
          </p>
          <p className="text-xs text-text/50 mt-1 max-w-xs">
            {t("settings.postProcessing.actions.empty")}
          </p>
          <Button
            variant="primary"
            size="md"
            onClick={() => setEditingId("new")}
            className="mt-4 flex items-center gap-1.5"
          >
            <Plus className="w-4 h-4" />
            {t("settings.postProcessing.actions.newAction")}
          </Button>
        </div>
      ) : (
        <div className="space-y-2">
          {actions.map((action) => (
            <ActionCard
              key={action.id}
              action={action}
              modelLabel={modelLabel(action.llm_model_id)}
              shortcut={actionShortcut(action.id)}
              onClick={() => setEditingId(action.id)}
            />
          ))}
        </div>
      )}

      {dialogOpen && (
        <ActionDialog
          key={editingId ?? "new"}
          open={dialogOpen}
          action={editingAction}
          onClose={() => setEditingId(null)}
          onSaved={(id) => setEditingId(id)}
        />
      )}
    </div>
  );
};
