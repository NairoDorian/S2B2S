import type { JSX } from "@solidjs/web";
interface SettingsGroupProps {
  title?: string;
  description?: string;
  children: JSX.Element;
}

export const SettingsGroup = (props: SettingsGroupProps): JSX.Element => {
  return (
    <div class="space-y-2">
      {props.title && (
        <div class="px-4">
          <h2 class="text-xs font-medium text-mid-gray uppercase tracking-wide">
            {props.title}
          </h2>
          {props.description && (
            <p class="text-xs text-mid-gray mt-1">{props.description}</p>
          )}
        </div>
      )}
      <div class="bg-background border border-mid-gray/20 rounded-lg overflow-visible">
        <div class="divide-y divide-mid-gray/20">{props.children}</div>
      </div>
    </div>
  );
};
