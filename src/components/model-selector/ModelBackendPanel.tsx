/* oxlint-disable jsx-a11y/prefer-tag-over-role */
import { For, Show, createSignal } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Check } from "@/components/icons/lucide";
import { commands } from "@/bindings";
import type { ModelBackendSetting } from "@/bindings";
import type { JSX } from "@solidjs/web";
import type { TFunction } from "i18next";

/**
 * The per-model backend choice, in the order it is offered.
 *
 * `auto` first because it is the default and the only one that defers to the
 * global accelerator setting; `cpu` next because it is the one a user reaches
 * for to take a model off the GPU. The GPU backends follow, and are filtered
 * against what this build can actually load a model on — so the list is
 * honest about the binary it is running in, not about what the engine
 * supports in principle.
 */
export const BACKEND_ORDER: ModelBackendSetting[] = [
  "auto",
  "cpu",
  "cuda",
  "vulkan",
  "metal",
  "rocm",
];

/** Product names, deliberately not translated: CUDA, Vulkan and Metal are what
 *  the user's driver is called, and "ROCm" is AMD's spelling. Only the two
 *  words around them are i18n keys. */
const BACKEND_LITERAL: Partial<Record<ModelBackendSetting, string>> = {
  cuda: "CUDA",
  vulkan: "Vulkan",
  metal: "Metal",
  rocm: "ROCm",
};

export const backendLabel = (
  backend: ModelBackendSetting,
  t: TFunction,
): string => BACKEND_LITERAL[backend] ?? t(`modelSelector.backend.${backend}`);

/**
 * Available per-model backends for this process, shared by every selector so
 * the app asks the native side once per session rather than once per model row.
 */
let backendsPromise: Promise<ModelBackendSetting[]> | null = null;

export const loadAvailableBackends = (): Promise<ModelBackendSetting[]> => {
  backendsPromise ??= commands
    .getAvailableAccelerators()
    .then(
      (available) =>
        available.model_backends.filter((name) =>
          (BACKEND_ORDER as string[]).includes(name),
        ) as ModelBackendSetting[],
    )
    .catch(() => BACKEND_ORDER);
  return backendsPromise;
};

interface ModelBackendPanelProps {
  /** The model whose backend is being chosen; `null` disables the panel. */
  modelId: string | null;
  /** Current choice for that model (absent = `auto`). */
  selected: ModelBackendSetting;
  onSelect: (backend: ModelBackendSetting) => void;
}

/**
 * Radio list of the backends one model may run on.
 *
 * Rendered inside the status-bar popover for the primary model. The Multi-STT
 * slots use `ModelBackendDropdown` instead, which shares
 * `loadAvailableBackends` / `backendLabel` and writes the same map through
 * `settingsStore.setModelBackend`.
 */
export const ModelBackendPanel = (
  props: ModelBackendPanelProps,
): JSX.Element => {
  const { t } = useTranslation();
  const [backends, setBackends] =
    createSignal<ModelBackendSetting[]>(BACKEND_ORDER);

  // The list starts complete so the panel is never empty, then narrows to what
  // this build can actually load on. `loadAvailableBackends` memoises the IPC,
  // so this costs one round trip per session however many panels exist.
  void loadAvailableBackends().then((names) => {
    // Keep the guaranteed pair even if the native list is somehow shorter,
    // so a stored value is never unselectable.
    setBackends(
      BACKEND_ORDER.filter(
        (backend) =>
          names.includes(backend) || backend === "auto" || backend === "cpu",
      ),
    );
  });

  return (
    <ul role="radiogroup" class="py-1">
      <For each={backends()}>
        {(backend) => {
          // An accessor: the row outlives a change of selection.
          const isSelected = () => backend === props.selected;
          return (
            <li>
              <button
                type="button"
                role="radio"
                aria-checked={isSelected() ? "true" : "false"}
                disabled={props.modelId === null}
                onClick={() => props.onSelect(backend)}
                class={`mx-1 flex w-[calc(100%-0.5rem)] items-start gap-2 rounded-md px-2 py-1.5 text-start transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${isSelected() ? "bg-accent/10" : "hover:bg-mid-gray/10"}`}
              >
                <Check
                  class={`mt-0.5 h-3 w-3 shrink-0 ${isSelected() ? "text-accent" : "text-transparent"}`}
                />
                <span class="min-w-0">
                  <span
                    class={`block font-medium ${isSelected() ? "text-accent" : "text-text/85"}`}
                  >
                    {backendLabel(backend, t)}
                  </span>
                  <span class="block text-[11px] leading-snug text-text/45">
                    {t(`modelSelector.backend.descriptions.${backend}`)}
                  </span>
                </span>
              </button>
            </li>
          );
        }}
      </For>
      <Show when={props.selected !== "auto"}>
        <li class="border-t border-mid-gray/20 px-2 py-1.5 text-[11px] leading-snug text-text/45">
          {t("modelSelector.backend.appliesOnNextLoad")}
        </li>
      </Show>
    </ul>
  );
};

export default ModelBackendPanel;
