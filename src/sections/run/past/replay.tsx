/* WF-23: treść podglądu kuratoruje Rust; historia nie odtwarza grafu ani uprawnień. */
import { useState, type ReactElement } from 'react';
import { why } from '../../../ipc/why';
import { copyRecordedWorkflow, prepareReplay, startReplay, type ReplayPreview } from '../io';
import { closeHistory } from './store';

export function Replay({
  project,
  runFolder,
}: {
  project: string;
  runFolder: string;
}): ReactElement {
  const [preview, setPreview] = useState<ReplayPreview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function prepare(mode: ReplayPreview['mode']): Promise<void> {
    const source = runFolder.split('__').at(-1);
    if (source === undefined) return;
    setBusy(true);
    setProblem(null);
    setPreview(null);
    try {
      setPreview(await prepareReplay(project, source, mode));
    } catch (error: unknown) {
      setProblem(why(error, 'This saved setup could not be read.'));
    } finally {
      setBusy(false);
    }
  }
  async function start(): Promise<void> {
    if (preview === null) return;
    setBusy(true);
    setProblem(null);
    try {
      await startReplay(project, preview.previewId);
      closeHistory();
    } catch (error: unknown) {
      setProblem(why(error, 'Nothing started. Review this repeat again.'));
    } finally {
      setBusy(false);
    }
  }
  async function copy(): Promise<void> {
    const source = runFolder.split('__').at(-1);
    if (source === undefined) return;
    setBusy(true);
    setProblem(null);
    setCopied(null);
    try {
      setCopied((await copyRecordedWorkflow(project, source)).said);
    } catch (error: unknown) {
      setProblem(why(error, 'The saved workflow could not be copied. Nothing was overwritten.'));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="mt-4 grid gap-2 px-[18px]">
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          className="btn-quiet"
          disabled={busy}
          onClick={() => {
            void copy();
          }}
        >
          Save workflow as a new copy
        </button>
        <button
          type="button"
          className="btn-quiet"
          disabled={busy}
          onClick={() => {
            void prepare('recorded');
          }}
        >
          Repeat saved setup
        </button>
        <button
          type="button"
          className="btn-quiet"
          disabled={busy}
          onClick={() => {
            void prepare('current');
          }}
        >
          Repeat current setup
        </button>
      </div>
      {copied !== null && <p className="lead">{copied}</p>}
      {preview !== null && (
        <section aria-label="Review repeat" className="card grid gap-2 p-3">
          <p className="lead">{preview.said}</p>
          {preview.budgetSaid !== undefined && <p className="lead">{preview.budgetSaid}</p>}
          {(preview.differencesSaid ?? []).map((said) => (
            <p key={said} className="label">
              {said}
            </p>
          ))}
          {(preview.configurationSaid ?? []).length > 0 && (
            <details>
              <summary className="label">Agent settings</summary>
              {(preview.configurationSaid ?? []).map((said, index) => (
                <p key={index} className="label">
                  {said}
                </p>
              ))}
            </details>
          )}
          {preview.limitations.map((one) => (
            <p key={one} className="label">
              {one}
            </p>
          ))}
          <div className="flex gap-2">
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => {
                void start();
              }}
            >
              Start replay
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
      {problem !== null && (
        <p role="alert" className="lead">
          {problem}
        </p>
      )}
    </section>
  );
}
