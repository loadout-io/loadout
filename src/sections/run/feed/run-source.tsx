import type { ReactElement } from 'react';
import type { RunSource } from '../../../ipc/types';
import { openOneRun } from '../history-command';
import { showHistory } from '../past/store';

/** Odnośnik jest odczytem. Nie startuje ani nie zatrzymuje niczego z historycznej treści. */
export function openRunSource(source: RunSource): Promise<void> {
  showHistory(source.workspace, []);
  return openOneRun(source.workspace, source.runFolder);
}

export function RunSourceControl({ source }: { source: RunSource }): ReactElement {
  return (
    <button
      type="button"
      className="btn-quiet mt-1"
      title={`Read on ${source.observedAt}`}
      onClick={() => {
        void openRunSource(source);
      }}
    >
      Open run source
    </button>
  );
}
