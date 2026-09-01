/* Co powstało z tego biegu — i jak to wziąć.
 *
 * # Po co to istnieje
 *
 * Bieg kończył się i milczał. Praca stała na gałęziach `loadout/<bieg>/<krok>`, o których produkt
 * nie mówił ani słowa: żeby ją zobaczyć, trzeba było wiedzieć, że istnieją, i znać schemat nazwy.
 * To jest brakujące ostatnie zdanie ścieżki: „bieg skończony, oto co powstało, weź to".
 *
 * # Dlaczego przycisk, a nie automat
 *
 * `FOUNDATIONS §2.1` rozdziela to, co powiedział agent, to, co znalazły sprawdzenia, i to, co
 * ZATWIERDZIŁ człowiek. Gałąź wyniku jest propozycją; złożenie jej naciska człowiek, a wzięcie
 * jej na swoją gałąź to jego `git merge`.
 */
import type { ReactElement } from 'react';
import { useState } from 'react';

import type { Landing } from './io';
import { foldRun, proposedName } from './io';

export interface WhatCameOutProps {
  /** Bieg, którego pracę składamy. */
  readonly run: string;
  /** Wstrzykiwane w kryteriach; produkcyjnie idzie przez IPC. */
  readonly ask?: typeof proposedName;
  readonly fold?: typeof foldRun;
}

/** Zdanie, które człowiek czyta zamiast surowego wyniku. */
export function said(landing: Landing | string): string {
  if (typeof landing === 'string') return landing;
  switch (landing.kind) {
    case 'landed':
      return `The work of ${String(landing.steps)} step${landing.steps === 1 ? '' : 's'} is on ${landing.branch}. Merge it when you are ready.`;
    case 'nothing':
      return 'No step in this run changed a file, so there is nothing to bring together.';
    case 'clash':
      /* Pliki są tu treścią, nie ozdobą: „dwa kroki się nie zgadzają" bez nazwy pliku zostawia
       * człowieka z pytaniem, na które ta odpowiedź miała odpowiedzieć. */
      return `Two steps wrote the same files, so nothing was created. ${landing.with} disagrees on: ${landing.files.join(', ')}.`;
  }
}

export function WhatCameOut({
  run,
  ask = proposedName,
  fold = foldRun,
}: WhatCameOutProps): ReactElement {
  const [id, setId] = useState('');
  const [proposal, setProposal] = useState({
    name: '',
    convention: null as string | null,
    taken: false,
  });
  const [answer, setAnswer] = useState<string | null>(null);
  const [going, setGoing] = useState(false);

  const ready = proposal.name !== '' && !proposal.taken && !going;

  return (
    <section data-what-came-out className="stack pane enter gap-2 p-4">
      <h2 className="text-title">What came out</h2>
      <label htmlFor="result-branch-id" className="label">
        Task id
      </label>
      <input
        id="result-branch-id"
        data-branch-id
        value={id}
        placeholder="T-160"
        onChange={(event) => {
          const typed = event.target.value;
          setId(typed);
          setAnswer(null);
          void ask(typed).then(setProposal);
        }}
        className="field"
      />
      {/* NAZWA WIDOCZNA ZANIM COKOLWIEK POWSTANIE. Propozycja, której człowiek nie widzi przed
          naciśnięciem, jest zgadywaniem, którego skutek poznaje po fakcie — a nazwy gałęzi nie
          poprawia się jednym kliknięciem. */}
      {proposal.name === '' ? null : (
        <p data-branch-preview className="label text-muted">
          {proposal.taken
            ? `There is already a branch called ${proposal.name}. Pick another id.`
            : `Will create ${proposal.name}${proposal.convention === null ? ' — this repository has no branch naming habit to follow' : ''}`}
        </p>
      )}
      <button
        type="button"
        data-fold-run
        disabled={!ready}
        onClick={() => {
          setGoing(true);
          void fold(run, proposal.name)
            .then((landing) => {
              setAnswer(said(landing));
            })
            .finally(() => {
              setGoing(false);
            });
        }}
        className="btn"
      >
        Bring it together
      </button>
      {answer === null ? null : (
        <p data-fold-said className="text-lede">
          {answer}
        </p>
      )}
    </section>
  );
}
