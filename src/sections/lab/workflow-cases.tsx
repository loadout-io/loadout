import type { ReactElement } from 'react';
import { useCallback, useState } from 'react';

import { why } from '../../ipc/why';
import type { LabState, useLab } from '../../state/lab';
import { newId } from '../workflows/io';
import type { EvalCase, EvalExpect } from './io';
import { ExaminerFields } from './examiner';
import { WorkflowInputs } from './workflow-inputs';

const FIELD = 'mt-1 w-full rounded-sm border border-line bg-well px-2 py-2 text-ui text-ink';
const BUTTON = 'h-8 rounded-sm border border-line px-3 text-ui text-body';

/** Zapis to szkic. Osobne istniejące Accept w Waiting for you decyduje o użyciu kryterium. */
export function WorkflowCases({
  state,
  store,
}: {
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement {
  const [adding, setAdding] = useState(false);
  const board = state.board;
  if (board === null) return <></>;
  const save = async (one: EvalCase): Promise<void> => {
    await store.getState().putCase(one);
    if (store.getState().said === null) setAdding(false);
  };
  return (
    <section data-lab-workflow-cases className="stack" data-gap="3">
      <h2 className="text-eyebrow">Cases and starting files</h2>
      <p className="max-w-160 lead">
        Accepted cases run in every column. New cases wait for you to accept them; saving does not
        start a comparison.
      </p>
      {board.set.set.cases
        .filter((one) => one.status === 'in-use')
        .map((one) => (
          <details key={one.id + ':' + board.set.revision} className="paper p-3">
            <summary className="text-ui text-ink cursor-pointer">{one.name}</summary>
            <CaseForm
              one={one}
              protectedFiles={board.set.set.protected === true}
              busy={state.busy !== 'idle'}
              onSave={save}
            />
          </details>
        ))}
      {adding ? (
        <CaseForm
          key={'new:' + board.set.revision}
          busy={state.busy !== 'idle'}
          protectedFiles={board.set.set.protected === true}
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
            disabled={state.busy !== 'idle'}
            onClick={() => {
              setAdding(true);
            }}
          >
            Add case
          </button>
        </div>
      )}
    </section>
  );
}

function expectedFields(raw: string): readonly EvalExpect[] {
  const value: unknown = JSON.parse(raw);
  if (
    !Array.isArray(value) ||
    !value.every(
      (one: unknown) =>
        typeof one === 'object' &&
        one !== null &&
        'field' in one &&
        typeof one.field === 'string' &&
        one.field.trim() !== '' &&
        'contains' in one &&
        typeof one.contains === 'string' &&
        'describe' in one &&
        typeof one.describe === 'string',
    )
  ) {
    throw new Error('Expected fields must be a list with field, contains and describe text.');
  }
  return value as EvalExpect[];
}

function CaseForm({
  one,
  busy,
  protectedFiles,
  onSave,
  onCancel,
}: {
  readonly one?: EvalCase;
  readonly busy: boolean;
  readonly protectedFiles: boolean;
  readonly onSave: (one: EvalCase) => Promise<void>;
  readonly onCancel?: () => void;
}): ReactElement {
  const [name, setName] = useState(one?.name ?? '');
  const [task, setTask] = useState(one?.task ?? '');
  const [sourceRun, setSourceRun] = useState(one?.input?.sourceRunId ?? '');
  const [snapshot, setSnapshot] = useState(one?.input?.snapshotId ?? '');
  const [inputReady, setInputReady] = useState(true);
  const [repeats, setRepeats] = useState(String(one?.repeats ?? 1));
  const [command, setCommand] = useState(one?.command ?? '');
  const [proof, setProof] = useState(one?.proof ?? '');
  const [proofMode, setProofMode] = useState(one?.proofMode ?? 'output-pattern');
  const [trusted, setTrusted] = useState(protectedFiles || one?.examiner !== undefined);
  const [program, setProgram] = useState(one?.examiner?.program ?? '');
  const [source, setSource] = useState(one?.examiner?.source ?? '');
  const [expect, setExpect] = useState(JSON.stringify(one?.expect ?? [], null, 2));
  const [dirty, setDirty] = useState(one === undefined);
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const selectInput = useCallback((input: EvalCase['input'], ready: boolean): void => {
    setSourceRun(input?.sourceRunId ?? '');
    setSnapshot(input?.snapshotId ?? '');
    setInputReady(ready);
    setDirty(true);
  }, []);
  const save = async (): Promise<void> => {
    try {
      if (!inputReady) throw new Error('Choose available starting files before saving this case.');
      const count = Number(repeats);
      if (!Number.isInteger(count) || count < 1 || count > 20)
        throw new Error('A case must run between 1 and 20 times.');
      if ((sourceRun.trim() === '') !== (snapshot.trim() === ''))
        throw new Error('Choose both the source run ID and saved input ID, or leave both empty.');
      const fields = expectedFields(expect);
      if (trusted) {
        if (!protectedFiles)
          throw new Error(
            'Save restricted file access for this set before saving trusted checker code.',
          );
        if (!program.trim().startsWith('/'))
          throw new Error('Choose an absolute path to a trusted Python interpreter.');
        if (source.trim() === '' || new TextEncoder().encode(source).byteLength > 256 * 1024)
          throw new Error('Trusted checker code must contain text within 256 KiB.');
      }
      setSaving(true);
      setProblem(null);
      const made: EvalCase = {
        ...one,
        id: one?.id ?? (await newId()),
        name: name.trim(),
        task: task.trim(),
        command: trusted ? '' : command.trim(),
        proof: trusted ? '' : proof.trim(),
        proofMode: trusted ? 'external-assessment-v1' : proofMode,
        repeats: count,
        expect: fields,
        // Zmiana zaakceptowanego sprawdzenia jest nową kandydatką, nie nową milczącą zgodą.
        status: trusted || one?.examiner !== undefined ? 'suggested' : (one?.status ?? 'suggested'),
        because: one?.because ?? 'Written by you in Lab',
      };
      // Usunięcie wyboru musi zdjąć stary adres, a nie zachować go przez spread.
      const { input: _previous, examiner: _previousExaminer, ...withoutPrevious } = made;
      const withoutInput: EvalCase = trusted
        ? { ...withoutPrevious, examiner: { kind: 'python', program: program.trim(), source } }
        : withoutPrevious;
      const withInput =
        sourceRun.trim() === ''
          ? withoutInput
          : {
              ...withoutInput,
              input: { sourceRunId: sourceRun.trim(), snapshotId: snapshot.trim() },
            };
      await onSave(withInput);
    } catch (error: unknown) {
      setProblem(why(error, 'Loadout could not save this case.'));
    } finally {
      setSaving(false);
    }
  };
  return (
    <form
      data-workflow-case={one?.id ?? 'new'}
      className="paper p-3 stack"
      data-gap="3"
      onChange={() => {
        setDirty(true);
      }}
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <fieldset disabled={busy || saving} className="grid gap-3 sm:grid-cols-2">
        <label className="label">
          Case name
          <input
            aria-label="Case name"
            required
            className={FIELD}
            value={name}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
        </label>
        <label className="label">
          Repeats
          <input
            aria-label="Repeats"
            type="number"
            min={1}
            max={20}
            required
            className={FIELD}
            value={repeats}
            onChange={(event) => {
              setRepeats(event.target.value);
            }}
          />
        </label>
        <label className="label sm:col-span-2">
          Task
          <textarea
            aria-label="Task"
            required
            rows={3}
            className={FIELD}
            value={task}
            onChange={(event) => {
              setTask(event.target.value);
            }}
          />
        </label>
        <WorkflowInputs sourceRun={sourceRun} snapshot={snapshot} onSelection={selectInput} />
        <label className="label sm:col-span-2">
          Check method
          <select
            aria-label="Check method"
            className={FIELD}
            value={trusted ? 'trusted' : 'diagnostic'}
            disabled={protectedFiles}
            onChange={(event) => {
              setTrusted(event.target.value === 'trusted');
            }}
          >
            <option value="diagnostic">Diagnostic command</option>
            <option value="trusted">Trusted external code</option>
          </select>
        </label>
        {trusted ? (
          <>
            <ExaminerFields
              program={program}
              source={source}
              onProgram={setProgram}
              onSource={setSource}
            />
            <p className="sm:col-span-2 text-note text-muted">
              Saving sends this case to Waiting for you. Review the code and accept it separately
              before it can run.
            </p>
          </>
        ) : (
          <>
            <p className="sm:col-span-2 text-note text-muted">
              Diagnostic checks do not provide restricted file access.
            </p>
            <label className="label sm:col-span-2">
              Check command
              <input
                aria-label="Check command"
                required
                className={FIELD}
                value={command}
                onChange={(event) => {
                  setCommand(event.target.value);
                }}
              />
            </label>
            <label className="label">
              Result format
              <select
                aria-label="Result format"
                className={FIELD}
                value={proofMode}
                onChange={(event) => {
                  setProofMode(
                    event.target.value === 'external-assessment-v1'
                      ? 'external-assessment-v1'
                      : 'output-pattern',
                  );
                }}
              >
                <option value="output-pattern">Passing output count</option>
                <option value="external-assessment-v1">External assessment v1</option>
              </select>
            </label>
            <label className="label">
              Passing output
              <input
                aria-label="Passing output"
                required
                className={FIELD}
                value={proof}
                onChange={(event) => {
                  setProof(event.target.value);
                }}
              />
            </label>
          </>
        )}
        <label className="label sm:col-span-2">
          Expected fields (JSON)
          <textarea
            aria-label="Expected fields (JSON)"
            rows={3}
            className={FIELD}
            value={expect}
            onChange={(event) => {
              setExpect(event.target.value);
            }}
          />
        </label>
      </fieldset>
      {problem === null ? null : (
        <p role="alert" className="lead" data-tone="attend">
          {problem}
        </p>
      )}
      <div className="flex gap-2">
        {dirty ? (
          <button type="submit" className={BUTTON} disabled={busy || saving || !inputReady}>
            Save case
          </button>
        ) : null}
        {onCancel === undefined ? null : (
          <button type="button" className={BUTTON} disabled={saving} onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}
