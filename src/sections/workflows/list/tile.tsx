/* Kafelek listy workflow (makieta `docs/mockup/index.html:642-650`).
 *
 * Kafelek pokazuje WYŁĄCZNIE to, co jest w pliku: nazwę, jedno zdanie opisu i wiersz
 * metadanych `N steps` · `M agents`, gdzie `M` liczy RÓŻNE identyfikatory agentów w krokach.
 * `used 12×` i `~6 min` z makiety wymagają historii biegów (T-06) i wchodzą razem z nią —
 * nigdy jako `—`, `never` ani `not reported`. Pole, które nigdy nie będzie miało treści,
 * zajmuje miejsce na ekranie i tłumaczy się użytkownikowi z własnej pustki; poprzedni prototyp
 * zostawił po sobie dokładnie taką komórkę `SPEND: not reported` (00-SYNTHESIS §6).
 *
 * Ten komponent nie dostaje żadnej akcji i to jest decyzja, nie przeoczenie: kontrolka bez
 * handlera nie wchodzi do repo (niezmiennik 16), a `Duplicate` i `Delete` mają handlery
 * jedno piętro wyżej, w `workflow-list.tsx`, gdzie mieszka obiekt `actions`. Kafelek zostaje
 * funkcją pliku.
 *
 * Z tego samego powodu kafelek nie jest `<button>`, choć makieta go tak rysuje: otwarcie
 * workflow na płótnie należy do T-13, a przycisk, który nic nie woła, na zrzucie ekranu
 * wygląda lepiej niż wersja poprawna. Wraca jako przycisk w tym samym commicie, w którym
 * pojawia się płótno, do którego prowadzi.
 */
import type { ReactElement } from 'react';
import type { WorkflowFile } from './store';

export interface WorkflowTileProps {
  wf: WorkflowFile;
}

/**
 * `1 step`, ale `4 steps`.
 *
 * Odmiana idzie za liczbą, a nie obok niej: `${n} steps` czyta się poprawnie przy czterech
 * i źle przy jednym, a napis wpisany na stałe czyta się poprawnie dokładnie na tym jednym
 * workflow, na którym ktoś go sprawdzał.
 */
function counted(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${count} ${noun}s`;
}

/**
 * Ilu RÓŻNYCH agentów robi tę robotę.
 *
 * Nie tyle, ile jest kroków: workflow z czterema krokami, w którym dwa robi ten sam agent,
 * ma dwóch agentów. Krok rodzaju `checkpoint` nie ma agenta i nie liczy się do niczego.
 */
function differentAgents(steps: WorkflowFile['steps']): number {
  const seen = new Set<string>();
  for (const step of steps) {
    if (step.agent !== undefined) {
      seen.add(step.agent);
    }
  }
  return seen.size;
}

export function WorkflowTile({ wf }: WorkflowTileProps): ReactElement {
  /* Pusty opis to brak opisu. Plik z `"description": ""` — a taki powstaje z jednego
   * skasowanego zdania — dałby zawsze renderowany akapit, czyli linijkę kafelka trzymaną
   * otwartą dla tekstu, którego tam nie ma. */
  const description = wf.description?.trim() ?? '';

  return (
    <article data-tile className="flex flex-col gap-2 rounded-sq border border-line bg-panel p-3">
      <h2 className="text-heading text-ink">{wf.name}</h2>

      {description === '' ? null : <p className="text-body text-muted">{description}</p>}

      {/* Liczby są wartościami maszynowymi, więc mono — reguła semantyczna z DESIGN §4.
       * Wiersz ma dokładnie dwie pozycje: obie są w pliku. Trzecia byłaby z historii biegów,
       * której v1 nie ma. */}
      <div className="flex gap-3 border-t border-line pt-2 font-mono text-mono text-muted">
        <span>{counted(wf.steps.length, 'step')}</span>
        <span>{counted(differentAgents(wf.steps), 'agent')}</span>
      </div>
    </article>
  );
}
