import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';

import { why } from '../../ipc/why';
import type { LabState, useLab } from '../../state/lab';
import { activeWorkspace } from '../../state/workspaces';
import type { Healthy } from '../../state/library';
import type { WorkflowEntry } from '../workflows/list/store';
import { listDefinitions, newId } from '../workflows/io';
import type { EvalVariant, EvalWorkflowSource } from './io';

const FIELD = 'mt-1 w-full rounded-sm border border-line bg-well px-2 py-2 text-ui text-ink';
const BUTTON = 'h-8 rounded-sm border border-line px-3 text-ui text-body';
type Choice = Healthy<WorkflowEntry>;
type Props = { readonly state: LabState; readonly store: typeof useLab };

/** WF-19: nie drugi edytor grafu. Kolumna wskazuje plik i jawne zmiany ustawień agentów. */
export function WorkflowColumns({ state, store }: Props): ReactElement {
  const [choices, setChoices] = useState<readonly Choice[]>([]);
  const [loading, setLoading] = useState(true);
  const [problem, setProblem] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [adding, setAdding] = useState(false);
  const folder = activeWorkspace()?.folder;
  useEffect(() => {
    let alive = true;
    setLoading(true);
    void listDefinitions()
      .then((definitions) => {
        if (!alive) return;
        setChoices(definitions.filter((one): one is Choice => one.kind === 'healthy'));
        setProblem(null);
        setLoading(false);
      })
      .catch((error: unknown) => {
        if (!alive) return;
        setProblem(why(error, 'Loadout could not read the workflows.'));
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [folder, refresh]);
  const board = state.board;
  if (board === null) return <></>;
  const columns = board.set.set.variants;
  const busy = state.busy !== 'idle';
  const save = async (variant: EvalVariant): Promise<void> => {
    await store.getState().putVariant(variant);
    if (store.getState().said === null) setAdding(false);
  };
  return (
    <section data-lab-workflow-columns className="stack" data-gap="3">
      <h2 className="text-eyebrow">Workflow columns</h2>
      <p className="max-w-160 lead">
        Each column runs a complete workflow. Select its source and the final step whose result will
        be checked.
      </p>
      {problem === null ? null : (
        <p role="alert" className="lead" data-tone="attend">
          {problem}
        </p>
      )}
      <div>
        <button
          type="button"
          className={BUTTON}
          disabled={loading || busy}
          onClick={() => {
            setRefresh((value) => value + 1);
          }}
        >
          Reload workflows
        </button>
      </div>
      {loading ? (
        <p className="lead">Reading workflows…</p>
      ) : choices.length === 0 ? (
        <p className="lead">
          No readable workflows are available. Save one in Workflows, then reload.
        </p>
      ) : (
        <>
          {columns.map((one) => (
            <ColumnForm
              key={one.id + ':' + board.set.revision}
              one={one}
              choices={choices}
              busy={busy}
              onSave={save}
              onRemove={
                columns.length < 2
                  ? undefined
                  : () => {
                      void store.getState().dropVariant(one.id);
                    }
              }
            />
          ))}
          {adding ? (
            <ColumnForm
              key={'new:' + board.set.revision}
              choices={choices}
              busy={busy}
              onSave={save}
              onCancel={() => {
                setAdding(false);
              }}
            />
          ) : (
            <div>
              <button
                type="button"
                className={BUTTON}
                disabled={busy}
                onClick={() => {
                  setAdding(true);
                }}
              >
                Add workflow column
              </button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

function keyOf(one: Choice): string {
  return one.value.place + '/' + one.value.path;
}
function initialChoice(one: EvalVariant | undefined, choices: readonly Choice[]): string {
  const source = one?.workflow;
  if (source?.place !== undefined && source.path !== undefined)
    return source.place + '/' + source.path;
  const matches = choices.filter((choice) => choice.value.workflow.id === source?.id);
  return matches.length === 1 && matches[0] !== undefined ? keyOf(matches[0]) : '';
}

function ColumnForm({
  one,
  choices,
  busy,
  onSave,
  onRemove,
  onCancel,
}: {
  readonly one?: EvalVariant;
  readonly choices: readonly Choice[];
  readonly busy: boolean;
  readonly onSave: (variant: EvalVariant) => Promise<void>;
  readonly onRemove?: (() => void) | undefined;
  readonly onCancel?: () => void;
}): ReactElement {
  const [source, setSource] = useState(() => initialChoice(one, choices));
  const selected = choices.find((choice) => keyOf(choice) === source);
  const [revision, setRevision] = useState(one?.workflow?.revision);
  const [name, setName] = useState(one?.name ?? 'New column');
  const [output, setOutput] = useState(one?.workflow?.outputStep ?? '');
  const [changes, setChanges] = useState(JSON.stringify(one?.workflow?.overrides ?? {}, null, 2));
  const [dirty, setDirty] = useState(one === undefined);
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const stale =
    revision !== undefined && selected?.revision !== undefined && revision !== selected.revision;
  const save = async (): Promise<void> => {
    if (selected === undefined || name.trim() === '' || output === '') return;
    try {
      const overrides: unknown = JSON.parse(changes);
      if (
        typeof overrides !== 'object' ||
        overrides === null ||
        Array.isArray(overrides) ||
        Object.values(overrides).some(
          (patch: unknown) => typeof patch !== 'object' || patch === null || Array.isArray(patch),
        )
      ) {
        throw new Error(
          'Step changes must be an object of step IDs and their changed agent settings.',
        );
      }
      const chosenRevision = revision ?? selected.revision;
      const workflow: EvalWorkflowSource = {
        id: selected.value.workflow.id,
        place: selected.value.place,
        path: selected.value.path,
        ...(chosenRevision === undefined ? {} : { revision: chosenRevision }),
        outputStep: output,
        overrides: overrides as EvalWorkflowSource['overrides'],
      };
      setSaving(true);
      setProblem(null);
      await onSave({
        ...one,
        id: one?.id ?? (await newId()),
        name: name.trim(),
        agent: one?.agent ?? '',
        overrides: one?.overrides ?? {},
        workflow,
      });
    } catch (error: unknown) {
      setProblem(why(error, 'Loadout could not save this column.'));
    } finally {
      setSaving(false);
    }
  };
  return (
    <form
      data-workflow-column={one?.id ?? 'new'}
      className="paper p-3 stack"
      data-gap="3"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <fieldset disabled={busy || saving} className="grid gap-3 sm:grid-cols-2">
        <label className="label">
          Column name
          <input
            aria-label="Column name"
            required
            className={FIELD}
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              setDirty(true);
            }}
          />
        </label>
        <label className="label">
          Workflow source
          <select
            aria-label="Workflow source"
            required
            className={FIELD}
            value={source}
            onChange={(event) => {
              const key = event.target.value;
              setSource(key);
              setOutput('');
              setRevision(choices.find((choice) => keyOf(choice) === key)?.revision);
              setDirty(true);
            }}
          >
            <option value="">Choose an exact source</option>
            {source !== '' && selected === undefined ? (
              <option value={source}>Unavailable · {source}</option>
            ) : null}
            {choices.map((choice) => (
              <option key={keyOf(choice)} value={keyOf(choice)}>
                {choice.value.workflow.name +
                  ' · ' +
                  (choice.value.place === 'project' ? 'Project' : 'Library') +
                  ' · ' +
                  choice.value.path}
              </option>
            ))}
          </select>
        </label>
        <label className="label">
          Output step
          <select
            aria-label="Output step"
            required
            className={FIELD}
            value={output}
            onChange={(event) => {
              setOutput(event.target.value);
              setDirty(true);
            }}
          >
            <option value="">Choose the final result</option>
            {selected?.value.workflow.steps
              .filter((step) => step.kind === 'agent' || step.kind === 'check')
              .map((step) => (
                <option key={step.id} value={step.id}>
                  {step.name + ' · ' + step.id}
                </option>
              ))}
          </select>
        </label>
        <div className="text-note text-muted self-end">
          {selected === undefined
            ? 'Select a source before saving.'
            : (revision ?? selected.revision) === undefined
              ? 'No saved revision is available. This column will use the file read at Start.'
              : 'Selected revision: ' + (revision ?? selected.revision)?.slice(0, 8)}
          {stale ? (
            <p>
              The file has changed since this column was saved.{' '}
              <button
                type="button"
                className="link"
                onClick={() => {
                  setRevision(selected?.revision);
                  setDirty(true);
                }}
              >
                Use current revision
              </button>
            </p>
          ) : null}
        </div>
        <label className="label sm:col-span-2">
          Step changes (JSON)
          <textarea
            aria-label="Step changes (JSON)"
            rows={3}
            className={FIELD}
            value={changes}
            onChange={(event) => {
              setChanges(event.target.value);
              setDirty(true);
            }}
          />
          <span className="text-note text-muted">
            Use step IDs and changed agent settings, for example {'{"build":{"model":"…"}}'}. The
            workflow file is not edited.
          </span>
        </label>
      </fieldset>
      {problem === null ? null : (
        <p role="alert" className="lead" data-tone="attend">
          {problem}
        </p>
      )}
      <div className="flex gap-2">
        {dirty ? (
          <button
            type="submit"
            className={BUTTON}
            disabled={
              busy || saving || selected === undefined || output === '' || name.trim() === ''
            }
          >
            Save column
          </button>
        ) : null}
        {onRemove === undefined ? null : (
          <button type="button" className={BUTTON} disabled={busy || saving} onClick={onRemove}>
            Remove column
          </button>
        )}
        {onCancel === undefined ? null : (
          <button type="button" className={BUTTON} disabled={saving} onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}
