import type { ReactElement } from 'react';

import type { EvalCase } from './io';

/* Kandydatka czekająca na człowieka — ten sam wzorzec, co notatka `suggested` w Memory.
 *
 * DWIE KONTROLKI I OBIE COŚ ROBIĄ (niezmiennik 16). „Accept" przestawia stan na `in-use`
 * i od tej chwili ten wiersz mierzy; „Discard" kasuje kandydatkę z pliku. Trzeciego stanu
 * nie ma i to jest wybór: notatka ma trzeci, bo jej odrzucenie NIESIE WIEDZĘ („tego już nie
 * proponuj"), a odrzucony przypadek nie niesie żadnej — lista zestawu ma zostać listą tego,
 * co mierzymy.
 *
 * POCHODZENIE STOI NA WIERZCHU, nie za kliknięciem. Człowiek ocenia kandydatkę wyłącznie po
 * tym, skąd ona jest: przypadek, który nie wskazuje pliku, jest przypadkiem wymyślonym,
 * a wymyślony przypadek mierzy wyobraźnię modelu. Rust takich nie zapisuje, więc to pole jest
 * tu zawsze — i dlatego wolno je pokazać bez warunku.
 */

export interface SuggestionProps {
  readonly one: EvalCase;
  readonly onKeep: (id: string) => void;
  readonly onDiscard: (id: string) => void;
  /** Poprawka komendy albo wzorca — jedyna rzecz, którą tu wolno zmienić. Powód niżej. */
  readonly onEdit: (one: EvalCase) => void;
  readonly busy: boolean;
}

export function Suggestion({
  one,
  onKeep,
  onDiscard,
  onEdit,
  busy,
}: SuggestionProps): ReactElement {
  return (
    <li data-lab-suggestion={one.id} className="border-b border-line-subtle p-3 last:border-b-0">
      <p className="text-body text-ink">{one.name}</p>
      <p className="mt-1 max-w-160 text-ui text-body">{one.task}</p>
      <p className="mt-1 text-ui text-muted">From {one.because}</p>
      {/* KOMENDA I WZORZEC SĄ DO POPRAWIENIA, i to jest jedyny moment, w którym człowiek
          realnie je poprawia: model proponuje komendę, ktora bywa o jeden przelacznik obok.
          Zadania ani oczekiwanych pol tu nie ma — te opisuja PRACE, a nie sposob jej
          sprawdzenia, i przypadek przepisany po przeczytaniu wyniku przestaje byc tym samym
          pomiarem. */}
      <div className="mt-2 flex gap-2">
        <input
          data-lab-case-command={one.id}
          aria-label="What command says whether this worked"
          placeholder="a command from this project, or nothing"
          defaultValue={one.command}
          disabled={busy}
          className="h-8 flex-1 rounded-sm border border-line bg-well px-2 text-ui text-ink"
          onBlur={(event) => {
            onEdit({ ...one, command: event.target.value.trim() });
          }}
        />
        <input
          data-lab-case-proof={one.id}
          aria-label="What has to appear in its output"
          placeholder="text that proves it ran"
          defaultValue={one.proof}
          disabled={busy}
          className="h-8 w-48 rounded-sm border border-line bg-well px-2 text-ui text-ink"
          onBlur={(event) => {
            onEdit({ ...one, proof: event.target.value.trim() });
          }}
        />
      </div>
      <div className="mt-2 flex gap-2">
        <button
          data-lab-keep={one.id}
          type="button"
          disabled={busy}
          className="h-8 rounded-sm bg-accent px-3 text-ui text-bg"
          onClick={() => {
            onKeep(one.id);
          }}
        >
          Accept
        </button>
        <button
          data-lab-discard={one.id}
          type="button"
          disabled={busy}
          className="h-8 rounded-sm border border-line px-3 text-ui text-body"
          onClick={() => {
            onDiscard(one.id);
          }}
        >
          Discard
        </button>
      </div>
    </li>
  );
}
