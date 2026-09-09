import type { ReactElement } from 'react';
import { useState } from 'react';

import type { LabState, useLab } from '../../state/lab';
import { Tick } from '../../ui/primitives/tick';
import { count } from './model';

/** Zmiana ustawienia nie jest ani zgodą na nowy kod sprawdzenia, ani poleceniem Start. */
export function WorkflowProtection({
  state,
  store,
}: {
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement {
  const saved = state.board?.set.set.protected === true;
  const [chosen, setChosen] = useState(saved);
  const preview = state.preview;
  const refusal = state.previewSaid ?? preview?.cannotRun ?? null;
  return (
    <section data-workflow-protection className="paper p-3 stack" data-gap="3">
      {/* NAGŁÓWEK MÓWI O NASTĘPNYM BIEGU, i to nie jest kosmetyka. Ten panel czyta zestaw
          zapisany DZIŚ, więc opisuje warunki, na których dopiero pobiegnie. Warunki, na
          których zmierzono wynik stojący wyżej, mówi zdanie przy macierzy
          (`model.howItWasJudged`) — a pod nagłówkiem „How results are judged" te dwa fakty
          czytały się jako jeden, i to ten fałszywy stał bliżej wyniku. */}
      <h2 className="text-eyebrow">How the next run will be judged</h2>
      <p className="lead">
        {saved
          ? 'File access is restricted; trusted external checks judge the result.'
          : 'Diagnostic comparison. Checks run outside the workflow, but file access is not restricted.'}
      </p>
      {state.board?.cannotRun !== null ? null : (
        <div data-workflow-preview className="stack" data-gap="2">
          {refusal !== null ? (
            <p className="lead" data-tone="attend">
              {refusal}
            </p>
          ) : preview?.size != null ? (
            <p className="text-ui text-ink">
              {count(preview.size.cells, 'comparison', 'comparisons') +
                ' · ' +
                count(preview.size.nodes, 'executed step', 'executed steps') +
                ' · ' +
                count(preview.size.edges, 'connection', 'connections') +
                ' · ' +
                count(preview.size.trees, 'working folder', 'working folders')}
            </p>
          ) : (
            <p className="text-ui text-muted">
              Checking the saved workflow and available agent applications…
            </p>
          )}
          <p className="text-note text-muted">
            Start checks availability again. This is not a sign-in or execution guarantee.
          </p>
          <div>
            <button
              type="button"
              className="h-8 rounded-sm border border-line px-3 text-ui text-body"
              disabled={state.busy !== 'idle'}
              onClick={() => {
                if (state.openId !== null) void store.getState().open(state.openId);
              }}
            >
              Check again
            </button>
          </div>
        </div>
      )}
      <Tick
        className="flex items-center gap-2 text-ui text-ink"
        label="Restrict file access"
        checked={chosen}
        disabled={state.busy !== 'idle'}
        onChange={(event) => {
          setChosen(event.target.checked);
        }}
      />
      <p className="max-w-160 text-note text-muted">
        Restricted comparisons require trusted checker code accepted separately for each case. If
        this setup cannot enforce file access restrictions, Start refuses; it does not switch to a
        diagnostic comparison.
      </p>
      {chosen === saved ? null : (
        <div className="flex items-center gap-3">
          <button
            type="button"
            className="h-8 rounded-sm border border-line px-3 text-ui text-body"
            disabled={state.busy !== 'idle'}
            onClick={() => {
              void store.getState().saveProtection(chosen);
            }}
          >
            Save evaluation scope
          </button>
          <span className="text-note text-muted">Not saved. This does not start a comparison.</span>
        </div>
      )}
    </section>
  );
}
