/* WF-24: wyłącznie renderer hostowego preview i rzeczywiste kliknięcia, bez zgody modelu. */
import { useState, type ReactElement } from 'react';
import { why } from '../../../ipc/why';
import {
  openRestoredFolder,
  prepareResultRestore,
  restoreResult,
  setResultKept,
  type PastRun,
  type RestorePreview,
} from '../io';

export function SavedResults({ project, run }: { project: string; run: PastRun }): ReactElement {
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [restored, setRestored] = useState<{
    readonly folder: string;
    readonly said: string;
  } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [kept, setKept] = useState<Readonly<Record<string, boolean>>>({});
  const [cleanup, setCleanup] = useState<{
    readonly resultId: string;
    readonly cleanupWarning: string;
  } | null>(null);
  const [keptSaid, setKeptSaid] = useState<string | null>(null);
  async function prepare(resultId: string): Promise<void> {
    const source = run.folder.split('__').at(-1);
    if (source === undefined) return;
    setBusy(true);
    setProblem(null);
    setPreview(null);
    setRestored(null);
    try {
      setPreview(await prepareResultRestore(project, source, resultId));
    } catch (error: unknown) {
      setProblem(why(error, 'The saved result could not be read.'));
    } finally {
      setBusy(false);
    }
  }
  async function restore(): Promise<void> {
    if (preview === null) return;
    setBusy(true);
    setProblem(null);
    try {
      setRestored(await restoreResult(project, preview.previewId, 'Restore files'));
      setPreview(null);
    } catch (error: unknown) {
      setProblem(why(error, 'The saved files could not be restored.'));
    } finally {
      setBusy(false);
    }
  }
  async function open(): Promise<void> {
    if (restored === null) return;
    try {
      await openRestoredFolder(project, restored.folder);
    } catch (error: unknown) {
      setProblem(why(error, 'The restored folder could not be opened.'));
    }
  }
  async function keep(resultId: string, value: boolean): Promise<void> {
    const source = run.folder.split('__').at(-1);
    if (source === undefined) return;
    setBusy(true);
    setProblem(null);
    setKeptSaid(null);
    try {
      const result = await setResultKept(
        project,
        source,
        resultId,
        value,
        value ? 'Keep result' : 'Allow cleanup of this result',
      );
      setKept((before) => ({ ...before, [resultId]: result.kept }));
      setKeptSaid(result.said);
      setCleanup(null);
    } catch (error: unknown) {
      setProblem(why(error, 'This result could not be updated.'));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="mt-4 grid gap-2 px-[18px]">
      {(run.savedResults ?? []).map((result) => (
        <div key={result.resultId} className="flex items-center gap-2">
          <span className="label">{result.name}</span>
          <button
            type="button"
            className="btn-quiet"
            disabled={busy || !result.available}
            onClick={() => {
              void prepare(result.resultId);
            }}
          >
            Restore saved files
          </button>
          {(kept[result.resultId] ?? result.kept) ? (
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => {
                setCleanup(result);
              }}
            >
              Allow cleanup
            </button>
          ) : (
            <button
              type="button"
              className="btn-quiet"
              disabled={busy || !result.available}
              onClick={() => {
                void keep(result.resultId, true);
              }}
            >
              Keep result
            </button>
          )}
        </div>
      ))}
      {cleanup !== null && (
        <section aria-label="Review result cleanup" className="card grid gap-2 p-3">
          <p className="lead">{cleanup.cleanupWarning}</p>
          <div className="flex gap-2">
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => {
                void keep(cleanup.resultId, false);
              }}
            >
              Allow cleanup of this result
            </button>
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => {
                setCleanup(null);
              }}
            >
              Keep this result
            </button>
          </div>
        </section>
      )}
      {keptSaid !== null && <p className="lead">{keptSaid}</p>}
      {preview !== null && (
        <section aria-label="Review saved files" className="card grid gap-2 p-3">
          <p className="lead">{preview.said}</p>
          <p className="label break-all">{preview.folder}</p>
          <details>
            <summary className="label">Saved files</summary>
            <ul>
              {preview.files.map((file) => (
                <li className="label" key={file}>
                  {file}
                </li>
              ))}
            </ul>
          </details>
          <div className="flex gap-2">
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => {
                void restore();
              }}
            >
              Restore files
            </button>
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => {
                setPreview(null);
              }}
            >
              Keep viewing history
            </button>
          </div>
        </section>
      )}
      {restored !== null && (
        <div className="grid gap-2">
          <p className="lead">{restored.said}</p>
          <button
            type="button"
            className="btn-quiet"
            onClick={() => {
              void open();
            }}
          >
            Open restored folder
          </button>
        </div>
      )}
      {problem !== null && (
        <p role="alert" className="lead">
          {problem}
        </p>
      )}
    </section>
  );
}
