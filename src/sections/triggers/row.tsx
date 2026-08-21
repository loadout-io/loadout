import type { ReactElement } from 'react';

import type { TriggerView } from '../../state/triggers';

export interface TriggerRowProps {
  readonly trigger: TriggerView;
  readonly onToggle: (slug: string, enabled: boolean) => Promise<void>;
}

/** Compileable shell for red-before-green. T-65's visible row is intentionally absent. */
export function TriggerRow({
  trigger: _trigger,
  onToggle: _onToggle,
}: TriggerRowProps): ReactElement {
  return <li data-trigger-row />;
}
