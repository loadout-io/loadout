import type { ReactElement } from 'react';

export const REFLECTION_LABEL = 'Learn from this run';

export interface ReflectionToggleProps {
  readonly enabled: boolean;
  readonly disabled?: boolean;
  readonly onChange: (enabled: boolean) => void;
}

/** The visible owner of the private post-run learning choice. */
export function ReflectionToggle({
  enabled,
  disabled = false,
  onChange,
}: ReflectionToggleProps): ReactElement {
  return (
    <label className="flex items-center gap-2 whitespace-nowrap text-ui text-ink">
      <input
        type="checkbox"
        checked={enabled}
        disabled={disabled}
        onChange={(event) => {
          onChange(event.target.checked);
        }}
      />
      <span>{REFLECTION_LABEL}</span>
    </label>
  );
}
