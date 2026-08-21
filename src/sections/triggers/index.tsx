import type { ComponentType, ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';

import { useTriggers } from '../../state/triggers';
import type { TriggersStore } from '../../state/triggers';
import { sectionEntry } from '../../ui/sections';
import type { Section } from '../../ui/sections';
import { TriggerRow } from './row';
import type { TriggerRowProps } from './row';

export interface TriggersScreenProps {
  readonly store?: TriggersStore;
  /** A test seam for proving that the visible switch and the store share one handler. */
  readonly row?: ComponentType<TriggerRowProps>;
}

export default function TriggersScreen({
  store = useTriggers,
  row: Row = TriggerRow,
}: TriggersScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  useEffect(() => {
    void store.getState().load();
  }, [store]);

  const toggle = (slug: string, enabled: boolean): Promise<void> =>
    store.getState().toggle(slug, enabled);
  const empty = sectionEntry('triggers' as Section).empty;

  return (
    <section data-triggers-screen className="flex h-full flex-col">
      <header className="flex h-13 items-center border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Triggers</h1>
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {state.said === null ? null : (
          <p className="mb-3 max-w-160 text-body text-attend">{state.said}</p>
        )}

        {state.triggers.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
              ◇
            </span>
            <p data-empty className="text-body text-ink">
              {empty}
            </p>
            <p className="max-w-120 text-center text-body text-muted">
              Add a trigger file to Loadout’s triggers folder to start watching for new work.
            </p>
          </div>
        ) : (
          <ul className="overflow-hidden rounded-md border border-line bg-panel">
            {state.triggers.map((trigger) => (
              <Row key={trigger.slug} trigger={trigger} onToggle={toggle} />
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
