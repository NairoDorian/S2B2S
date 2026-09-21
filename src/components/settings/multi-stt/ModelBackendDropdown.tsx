import { Show, createMemo, createSignal } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Dropdown, type DropdownOption } from "@/components/ui/Dropdown";
import { useSettings } from "@/hooks/useSettings";
import {
  backendLabel,
  loadAvailableBackends,
} from "@/components/model-selector/ModelBackendPanel";
import type { ModelBackendSetting } from "@/bindings";
import type { JSX } from "@solidjs/web";

/** The order the choices are offered in, `auto` (the default) first. */
const BACKEND_ORDER = [
  "auto",
  "cpu",
  "cuda",
  "vulkan",
  "metal",
  "rocm",
] as const;

interface ModelBackendDropdownProps {
  /** The model this row pins. `null` (no model chosen) renders nothing. */
  modelId: string | null;
  /** Small caption before the dropdown; the languages row uses the same shape. */
  label: string;
  class?: string;
}

/**
 * One model's backend choice as a compact dropdown, for the places a full
 * status-bar popover does not fit — the Multi-STT panel's model rows.
 *
 * Same setting, same command and same available list as `ModelBackendPanel`:
 * both write `per_model_backends[modelId]` through
 * `settingsStore.setModelBackend`, so choosing here is choosing there.
 */
export const ModelBackendDropdown = (
  props: ModelBackendDropdownProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, isUpdating, setModelBackend } = useSettings();

  // Two choices always exist, so the dropdown is never empty or momentarily
  // missing the current value while the native list is in flight.
  const [available, setAvailable] = createSignal<ModelBackendSetting[]>([
    "auto",
    "cpu",
  ]);
  void loadAvailableBackends().then((names) => setAvailable(names));

  const options = createMemo<DropdownOption[]>(() =>
    BACKEND_ORDER.filter((backend) => available().includes(backend)).map(
      (backend) => ({ value: backend, label: backendLabel(backend, t) }),
    ),
  );

  const selected = (): ModelBackendSetting =>
    (
      getSetting("per_model_backends") as
        | Record<string, ModelBackendSetting>
        | undefined
    )?.[props.modelId ?? ""] ?? "auto";

  return (
    <Show when={props.modelId}>
      {(modelId) => (
        <div class={`flex items-center gap-2 mt-1 ml-1 ${props.class ?? ""}`}>
          <label class="text-xs text-mid-gray/70 whitespace-nowrap">
            {props.label}
          </label>
          <Dropdown
            selectedValue={selected()}
            options={options()}
            onSelect={(value) => {
              void setModelBackend(modelId(), value as ModelBackendSetting);
            }}
            disabled={isUpdating("per_model_backends")}
            class="min-w-[140px]"
          />
        </div>
      )}
    </Show>
  );
};

export default ModelBackendDropdown;
