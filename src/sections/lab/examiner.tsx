import type { ReactElement } from 'react';

import type { EvalCase } from './io';

type Examiner = NonNullable<EvalCase['examiner']>;
const FIELD = 'mt-1 w-full rounded-sm border border-line bg-well px-2 py-2 text-ui text-ink';

export function ExaminerFields({
  program,
  source,
  onProgram,
  onSource,
}: {
  readonly program: string;
  readonly source: string;
  readonly onProgram: (program: string) => void;
  readonly onSource: (source: string) => void;
}): ReactElement {
  return (
    <>
      <label className="label sm:col-span-2">
        Python interpreter
        <input
          aria-label="Python interpreter"
          required
          className={FIELD}
          value={program}
          placeholder="An absolute path to a trusted Python interpreter"
          onChange={(event) => {
            onProgram(event.target.value);
          }}
        />
      </label>
      <label className="label sm:col-span-2">
        Trusted checker code
        <textarea
          aria-label="Trusted checker code"
          required
          rows={9}
          spellCheck={false}
          className={FIELD}
          value={source}
          onChange={(event) => {
            onSource(event.target.value);
          }}
        />
      </label>
      <p className="sm:col-span-2 text-note text-muted">
        This code runs outside the workflow with the selected Python interpreter in isolated mode.
        Inspect the result files or launch the subject separately and capture its output; never
        import subject code into this trusted interpreter. Maximum 256 KiB.
      </p>
      <details className="sm:col-span-2 text-note text-muted">
        <summary className="cursor-pointer text-ui text-ink">Checker input and result</summary>
        <p className="mt-2">Read one JSON object from standard input:</p>
        <pre className="mt-1 overflow-auto text-note">
          {
            '{ "format": 1, "results": [{ "nodeKey": "...", "status": "...", "output": "...", "error": null, "cause": null, "files": null }] }'
          }
        </pre>
        <p className="mt-2">
          When files are available, files contains root and snapshotId. Print exactly one JSON
          object with format: 1, status, passed, failed and reason. Status is completed,
          subject-cannot-load or infrastructure-failed; passed and failed are non-negative integer
          counts, and reason is text.
        </p>
        <p className="mt-2">
          Passing requires a successful finish, status completed, passed greater than 0 and failed
          equal to 0. Keep the result within 64 KiB; send diagnostics to standard error.
        </p>
      </details>
    </>
  );
}

/** Kod widoczny PRZED osobnym Accept; sam podpis nie jest przeglądem tego, co uruchomimy. */
export function ExaminerReview({ examiner }: { readonly examiner: Examiner }): ReactElement {
  return (
    <div className="mt-2 stack" data-gap="2">
      <p className="text-ui text-ink">Trusted external check · {examiner.program}</p>
      <pre
        data-examiner-source
        className="max-h-80 overflow-auto whitespace-pre-wrap rounded-sm border border-line bg-well p-2 text-note text-ink"
      >
        {examiner.source}
      </pre>
      <p className="text-note text-muted">
        Accept only code and an interpreter you trust. Accepting does not run it.
      </p>
    </div>
  );
}
