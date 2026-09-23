import { createEffect, createSignal } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown, type DropdownOption } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";
import { commands } from "@/bindings";
import type { TranscribeAcceleratorSetting } from "@/bindings";

interface AccelerationSelectorProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

function encodeTranscribeValue(
  accelerator: TranscribeAcceleratorSetting,
  gpuDevice: string | null,
): string {
  if (accelerator === "cpu") return "cpu";
  if (accelerator === "gpu")
    return gpuDevice === null ? "gpu" : `gpu:${gpuDevice}`;
  return "auto";
}

function decodeTranscribeValue(value: string): {
  accelerator: TranscribeAcceleratorSetting;
  gpuDevice: string | null;
} {
  if (value === "cpu") return { accelerator: "cpu", gpuDevice: null };
  if (value === "gpu") return { accelerator: "gpu", gpuDevice: null };
  if (value.startsWith("gpu:")) {
    return { accelerator: "gpu", gpuDevice: value.slice(4) };
  }
  return { accelerator: "auto", gpuDevice: null };
}

export const AccelerationSelector = (props: AccelerationSelectorProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const [transcribeOptions, setTranscribeOptions] = createSignal<
    DropdownOption[]
  >([]);

  createEffect(
    () => undefined,
    () => {
      commands.getAvailableAccelerators().then((available) => {
        const opts: DropdownOption[] = [];
        if (available.transcribe.includes("auto")) {
          opts.push({
            value: "auto",
            label: t("settings.advanced.acceleration.gpuDevice.auto"),
          });
        }

        if (available.transcribe.includes("gpu")) {
          opts.push({ value: "gpu", label: "GPU" });
          for (const dev of available.gpu_devices) {
            const vramLabel =
              dev.total_vram_mb >= 1024
                ? `${(dev.total_vram_mb / 1024).toFixed(1)} GB`
                : `${dev.total_vram_mb} MB`;
            opts.push({
              value: `gpu:${dev.id}`,
              label: `${dev.name} (${vramLabel})`,
            });
          }
        }

        if (available.transcribe.includes("cpu")) {
          opts.push({ value: "cpu", label: "CPU" });
        }
        setTranscribeOptions(opts);
      });
    },
  );

  // Accessors, not consts: the options load asynchronously after mount and
  // the stored choice changes with every pick.
  const currentAccelerator = () =>
    getSetting("transcribe_accelerator") ?? "auto";
  const currentTranscribe = () =>
    encodeTranscribeValue(
      currentAccelerator() as TranscribeAcceleratorSetting,
      (getSetting("transcribe_gpu_device") ?? null) as string | null,
    );
  const displayedTranscribe = () => {
    const options = transcribeOptions();
    const current = currentTranscribe();
    if (options.some((option) => option.value === current)) return current;
    if (
      currentAccelerator() === "gpu" &&
      options.some((option) => option.value === "gpu")
    )
      return "gpu";
    return options[0]?.value ?? null;
  };

  const handleTranscribeChange = async (value: string) => {
    const { accelerator, gpuDevice } = decodeTranscribeValue(value);
    await updateSetting("transcribe_accelerator", accelerator);
    await updateSetting("transcribe_gpu_device", gpuDevice);
  };

  return (
    <SettingContainer
      title={t("settings.advanced.acceleration.transcribe.title")}
      description={t("settings.advanced.acceleration.transcribe.description")}
      descriptionMode={props.descriptionMode ?? "tooltip"}
      grouped={props.grouped ?? false}
      layout="horizontal"
    >
      <Dropdown
        options={transcribeOptions()}
        selectedValue={displayedTranscribe()}
        onSelect={handleTranscribeChange}
        disabled={
          isUpdating("transcribe_accelerator") ||
          isUpdating("transcribe_gpu_device")
        }
      />
    </SettingContainer>
  );
};
