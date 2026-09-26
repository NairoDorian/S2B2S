import { createEffect, createSignal, For, Show } from "solid-js";
import { ChevronDown } from "@/components/icons/lucide";
import { useTranslation } from "@/i18n/useTranslation";
import type { JSX } from "@solidjs/web";

interface BenchmarkRunsSelectProps {
  /** Id of the trigger, so the visible `<label for>` can name it. */
  id: string;
  value: number;
  options: number[];
  disabled?: boolean;
  onChange: (value: number) => void;
}

/**
 * Run-count picker for the quantization benchmark: the "timed runs to
 * average" control shown under the method note.
 *
 * Two deliberate departures from a native `<select>`:
 *
 * - **Themed, not OS-drawn.** The native popup ignores the app's theme and
 *   lands on a white list of numbers inside a dark popover. This one is built
 *   from the app's own tokens (`bg-mid-gray/15`, `text-text`), so it follows
 *   whichever theme the window is in.
 * - **In flow, not floating.** The popover it lives in is `overflow-hidden`
 *   (so its children clip to the rounded corners), and an absolutely
 *   positioned menu would be cut off whenever the quant list is short. This
 *   menu is a block above the control instead: the dialog's bottom edge is
 *   anchored to the status bar, so opening it simply grows the popover
 *   upward, and nothing can be clipped.
 *
 * The menu shows every count at once rather than one column of them — with
 * nine options a grid stays one row tall and the choice is visible without
 * scrolling.
 */
export const BenchmarkRunsSelect = (
  props: BenchmarkRunsSelectProps,
): JSX.Element => {
  const { t } = useTranslation();
  const [open, setOpen] = createSignal(false);
  let rootRef: HTMLDivElement | null = null;

  // The compute tracks `open`; the apply owns the listeners and returns their
  // teardown, so closing removes them (the pattern `Dropdown` uses).
  createEffect(
    () => open(),
    (isOpen) => {
      if (!isOpen) return;
      const handlePointerDown = (event: MouseEvent) => {
        if (rootRef && !rootRef.contains(event.target as Node)) setOpen(false);
      };
      const handleKeyDown = (event: KeyboardEvent) => {
        if (event.key === "Escape") setOpen(false);
      };
      document.addEventListener("mousedown", handlePointerDown);
      document.addEventListener("keydown", handleKeyDown);
      return () => {
        document.removeEventListener("mousedown", handlePointerDown);
        document.removeEventListener("keydown", handleKeyDown);
      };
    },
  );

  const select = (value: number) => {
    setOpen(false);
    if (value !== props.value) props.onChange(value);
  };

  return (
    <div
      class="mt-1.5"
      ref={(el) => {
        rootRef = el;
      }}
    >
      <Show when={open() && !props.disabled}>
        <div class="mb-1.5 grid grid-cols-9 gap-1 rounded-md border border-mid-gray/30 bg-mid-gray/15 p-1">
          <For each={props.options}>
            {(count) => (
              <button
                type="button"
                aria-pressed={count === props.value ? "true" : "false"}
                aria-label={`${t("modelSelector.benchmark.runsLabel")}: ${count}`}
                onClick={() => select(count)}
                class={`min-w-0 rounded px-0.5 py-1 text-center text-[11px] tabular-nums transition-colors ${
                  count === props.value
                    ? "bg-accent/25 font-semibold text-accent"
                    : "text-text/70 hover:bg-mid-gray/25 hover:text-text"
                }`}
              >
                {count}
              </button>
            )}
          </For>
        </div>
      </Show>

      <div class="flex items-center justify-between gap-2">
        <label for={props.id} class="text-text/50">
          {t("modelSelector.benchmark.runsLabel")}
        </label>
        <button
          type="button"
          id={props.id}
          aria-haspopup="true"
          aria-expanded={open() ? "true" : "false"}
          disabled={props.disabled}
          onClick={() => setOpen(!open())}
          class="flex items-center gap-1 rounded-md border border-mid-gray/30 bg-mid-gray/20 px-1.5 py-0.5 text-[11px] tabular-nums text-text/85 transition-colors hover:border-accent/60 hover:text-text focus:border-accent focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span class="min-w-3 text-center">{props.value}</span>
          <ChevronDown
            class={`h-3 w-3 shrink-0 transition-transform ${open() ? "rotate-180" : ""}`}
          />
        </button>
      </div>
    </div>
  );
};

export default BenchmarkRunsSelect;
