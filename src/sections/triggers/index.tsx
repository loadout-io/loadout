import type { ComponentType, ReactElement } from 'react';
import { useSyncExternalStore } from 'react';

import { useTriggers } from '../../state/triggers';
import type { TriggersStore } from '../../state/triggers';
import { TriggerRow } from './row';
import type { TriggerRowProps } from './row';

export interface TriggersScreenProps {
  readonly store?: TriggersStore;
  /** A test seam for proving that the visible switch and the store share one handler. */
  readonly row?: ComponentType<TriggerRowProps>;
}

/** Compileable screen scaffold. Its rows are deliberately empty until T-65 is implemented. */
export default function TriggersScreen({
  store = useTriggers,
  row: Row = TriggerRow,
}: TriggersScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const toggle = (slug: string, enabled: boolean): Promise<void> =>
    store.getState().toggle(slug, enabled);

  return (
    <section data-triggers-screen>
      <ul>
        {state.triggers.map((trigger) => (
          <Row key={trigger.slug} trigger={trigger} onToggle={toggle} />
        ))}
      </ul>
    </section>
  );
}
