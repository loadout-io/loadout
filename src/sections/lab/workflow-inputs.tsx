import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';

import { why } from '../../ipc/why';
import { activeWorkspace } from '../../state/workspaces';
import type { PastRunRow } from '../run/io';
import { listRuns, readRun } from '../run/io';
import type { EvalCase } from './io';

const FIELD = 'mt-1 w-full rounded-sm border border-line bg-well px-2 py-2 text-ui text-ink';
type Input = NonNullable<EvalCase['input']>;
type Mode = 'current' | 'saved' | 'advanced';

/** Odczyt dopiero wybranego biegu. Lista nie otwiera wszystkich logów ani całych wejść. */
export function WorkflowInputs({
  sourceRun,
  snapshot,
  onSelection,
}: {
  readonly sourceRun: string;
  readonly snapshot: string;
  readonly onSelection: (input: Input | undefined, ready: boolean) => void;
}): ReactElement {
  const [mode, setMode] = useState<Mode>(sourceRun === '' ? 'current' : 'advanced');
  const [runs, setRuns] = useState<readonly PastRunRow[]>([]);
  const [selected, setSelected] = useState('');
  const [reload, setReload] = useState(0);
  const [loading, setLoading] = useState(false);
  const [reading, setReading] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const folder = activeWorkspace()?.folder ?? null;
  useEffect(() => {
    if (mode !== 'saved') return;
    let alive = true;
    setLoading(true);
    setProblem(null);
    void listRuns(folder)
      .then((rows) => {
        if (!alive) return;
        setRuns(rows);
        setLoading(false);
      })
      .catch((error: unknown) => {
        if (!alive) return;
        setRuns([]);
        setLoading(false);
        setProblem(why(error, 'Loadout could not read the earlier runs.'));
      });
    return () => {
      alive = false;
    };
  }, [folder, mode, reload]);
  useEffect(() => {
    if (mode !== 'saved' || selected === '') return;
    let alive = true;
    setReading(true);
    setProblem(null);
    onSelection(undefined, false);
    void readRun(folder, selected)
      .then((run) => {
        if (!alive) return;
        const input = run.savedInput;
        if (
          run.folder !== selected ||
          input == null ||
          input.sourceRunId.trim() === '' ||
          input.snapshotId.trim() === ''
        ) {
          setProblem(
            run.savedInputSaid ?? 'These starting files are unavailable. Choose another run.',
          );
          onSelection(undefined, false);
        } else {
          onSelection(input, true);
        }
        setReading(false);
      })
      .catch((error: unknown) => {
        if (!alive) return;
        setReading(false);
        setProblem(why(error, 'Loadout could not verify these starting files.'));
        onSelection(undefined, false);
      });
    return () => {
      alive = false;
    };
  }, [folder, mode, selected, reload, onSelection]);
  return (
    <div className="sm:col-span-2 stack" data-gap="2">
      <label className="label">
        Starting files
        <select
          aria-label="Starting files"
          className={FIELD}
          value={mode}
          onChange={(event) => {
            const chosen = event.target.value as Mode;
            setMode(chosen);
            setSelected('');
            setProblem(null);
            setReading(false);
            onSelection(undefined, chosen === 'current');
          }}
        >
          <option value="current">This project at Start</option>
          <option value="saved">Starting files from an earlier run</option>
          <option value="advanced">Advanced: saved input IDs</option>
        </select>
      </label>
      {mode === 'current' ? (
        <p className="text-note text-muted">
          Every column starts from the same files captured when this comparison starts.
        </p>
      ) : null}
      {mode === 'saved' ? (
        <>
          <label className="label">
            Earlier run
            <select
              aria-label="Earlier run"
              className={FIELD}
              value={selected}
              disabled={loading}
              onChange={(event) => {
                setSelected(event.target.value);
                setProblem(null);
                onSelection(undefined, false);
              }}
            >
              <option value="">
                {loading ? 'Reading earlier runs…' : 'Choose a run by its name and date'}
              </option>
              {runs.map((run) => (
                <option key={run.folder} value={run.folder} disabled={run.said !== null}>
                  {(run.title || 'Unnamed run') +
                    ' · ' +
                    run.when +
                    (run.said === null ? '' : ' · record unavailable')}
                </option>
              ))}
            </select>
          </label>
          {!loading && runs.length === 0 && problem === null ? (
            <p className="text-note text-muted">There are no earlier runs in this project.</p>
          ) : null}
          <div>
            <button
              type="button"
              className="h-8 rounded-sm border border-line px-3 text-ui text-body"
              disabled={loading || reading}
              onClick={() => {
                onSelection(undefined, false);
                setReload((value) => value + 1);
              }}
            >
              Reload earlier runs
            </button>
          </div>
          {reading ? (
            <p className="text-note text-muted">Checking the saved starting files…</p>
          ) : snapshot !== '' && problem === null ? (
            <p className="text-note text-muted">
              Saved starting files verified. They are checked again at Start.
            </p>
          ) : null}
        </>
      ) : null}
      {mode === 'advanced' ? (
        <>
          <p className="text-note text-muted">
            Enter a known address from this project. These IDs are not verified here; Start refuses
            missing or changed files.
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="label">
              Source run ID
              <input
                aria-label="Source run ID"
                className={FIELD}
                value={sourceRun}
                onChange={(event) => {
                  const value = event.target.value;
                  onSelection(
                    { sourceRunId: value, snapshotId: snapshot },
                    value.trim() !== '' && snapshot.trim() !== '',
                  );
                }}
              />
            </label>
            <label className="label">
              Saved input ID
              <input
                aria-label="Saved input ID"
                className={FIELD}
                value={snapshot}
                onChange={(event) => {
                  const value = event.target.value;
                  onSelection(
                    { sourceRunId: sourceRun, snapshotId: value },
                    sourceRun.trim() !== '' && value.trim() !== '',
                  );
                }}
              />
            </label>
          </div>
        </>
      ) : null}
      {problem === null ? null : (
        <p role="alert" className="lead" data-tone="attend">
          {problem}
        </p>
      )}
    </div>
  );
}
