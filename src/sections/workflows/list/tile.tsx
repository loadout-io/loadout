/* Kafelek listy workflow (makieta `docs/mockup/index.html:642-650`).
 *
 * Kafelek pokazuje WYŁĄCZNIE to, co jest w pliku: nazwę, jedno zdanie opisu i wiersz
 * metadanych `N steps` · `M agents`, gdzie `M` liczy RÓŻNE identyfikatory agentów w krokach.
 * `used 12×` i `~6 min` z makiety wymagają historii biegów (T-06) i wchodzą razem z nią —
 * nigdy jako `—`, `never` ani `not reported`. Pole, które nigdy nie będzie miało treści,
 * zajmuje miejsce na ekranie i tłumaczy się użytkownikowi z własnej pustki; poprzedni prototyp
 * zostawił po sobie dokładnie taką komórkę `SPEND: not reported` (00-SYNTHESIS §6).
 *
 * `Duplicate` i `Delete` mają handlery jedno piętro wyżej, w `workflow-list.tsx`, gdzie mieszka
 * obiekt `actions`. Kafelek zostaje funkcją pliku plus jedną rzeczą: OTWARCIEM.
 *
 * 2026-08-18 — KAFELEK JEST PRZYCISKIEM. Do tego dnia był `<article>`, a otwarcie mieszkało
 * w trzecim szarym przycisku pod kartą; makieta otwiera workflow kliknięciem w kafelek
 * (`<button class="tile" data-go="flows">`) i to jest cała różnica między listą, po której się
 * klika, a listą, którą trzeba przeczytać, żeby znaleźć właściwy przycisk. Nagłówek tego pliku
 * obiecywał wprost, że kafelek „wraca jako przycisk w tym samym commicie, w którym pojawia się
 * płótno, do którego prowadzi" — płótno pojawiło się 2026-08-17, przycisk spóźnił się o dzień.
 *
 * 2026-08-18 — LICZNIK AGENTÓW NIE LICZY PUSTYCH. `seen.add(step.agent)` bez warunku dawało
 * zbiorowi z jednym pustym napisem rozmiar 1, więc nad plikiem, w którym ŻADEN krok nie ma
 * agenta, stało „2 steps 1 agent". To jedyne miejsce, w którym człowiek sprawdza kompletność
 * jednym spojrzeniem, i mówiło mu dokładną odwrotność prawdy. Zmierzone na dwóch plikach
 * właściciela: oba miały `"agent": ""` przy każdym kroku.
 */
import type { ReactElement } from 'react';
import type { WorkflowFile } from './store';

export interface WorkflowTileProps {
  wf: WorkflowFile;
  /**
   * Otwarcie tego workflow w edytorze.
   *
   * Wymagany, nie opcjonalny: kafelek JEST kontrolką otwarcia, a kontrolka bez handlera nie
   * wchodzi do repo (niezmiennik 16). Wariant „przycisk, kiedy handler jest, `<article>`, kiedy
   * go nie ma" dałby dwa kształty tej samej karty i pierwszą okazję, żeby na jakimś ekranie
   * wylądowała ta bez wyjścia.
   */
  onOpen: () => void;
}

/**
 * `1 step`, ale `4 steps`.
 *
 * Odmiana idzie za liczbą, a nie obok niej: `${n} steps` czyta się poprawnie przy czterech
 * i źle przy jednym, a napis wpisany na stałe czyta się poprawnie dokładnie na tym jednym
 * workflow, na którym ktoś go sprawdzał.
 */
function counted(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${String(count)} ${noun}s`;
}

/** Czy ten krok naprawdę kogoś nazywa.
 *
 * `trim()`, nie `!== ''`: `freshStep` daje `agent: ''`, ale plik poprawiony ręcznie potrafi mieć
 * tam spację, a spacja nie jest identyfikatorem agenta bardziej niż pusty napis. */
function names(agent: string): boolean {
  return agent.trim() !== '';
}

/**
 * Ilu RÓŻNYCH agentów robi tę robotę.
 *
 * Nie tyle, ile jest kroków: workflow z czterema krokami, w którym dwa robi ten sam agent,
 * ma dwóch agentów. Krok rodzaju `checkpoint` nie ma agenta i nie liczy się do niczego —
 * i krok, który agenta jeszcze nie wybrał, też nie (patrz nagłówek pliku).
 */
function differentAgents(steps: WorkflowFile['steps']): number {
  const seen = new Set<string>();
  for (const step of steps) {
    /* Po RODZAJU kroku, a nie po obecności pola: `agent` jest wymagany na kroku agenta
     * i nie istnieje na punkcie kontrolnym, więc rodzaj jest tym, co naprawdę rozstrzyga
     * (2026-08-17: schemat przyjechał z `src/state/workflows.ts`, gdzie to są dwa typy). */
    if (step.kind === 'agent' && names(step.agent)) {
      seen.add(step.agent);
    }
  }
  return seen.size;
}

/** Ile kroków agenta czeka na wybór agenta. Zero znaczy „nie ma o czym mówić". */
function waitingForAnAgent(steps: WorkflowFile['steps']): number {
  return steps.filter((step) => step.kind === 'agent' && !names(step.agent)).length;
}

export function WorkflowTile({ wf, onOpen }: WorkflowTileProps): ReactElement {
  /* Pusty opis to brak opisu. Plik z `"description": ""` — a taki powstaje z jednego
   * skasowanego zdania — dałby zawsze renderowany akapit, czyli linijkę kafelka trzymaną
   * otwartą dla tekstu, którego tam nie ma. */
  const description = wf.description?.trim() ?? '';
  const waiting = waitingForAnAgent(wf.steps);

  return (
    <button
      data-tile
      type="button"
      onClick={onOpen}
      className="flex flex-col gap-2 rounded-md border border-line bg-panel p-3 text-left hover:border-line-strong"
    >
      <span className="text-heading text-ink">{wf.name}</span>

      {description === '' ? null : <span className="text-body text-muted">{description}</span>}

      {/* Liczby są wartościami maszynowymi, więc mono — reguła semantyczna z DESIGN §4.
       * Dwie pierwsze pozycje są w pliku. Trzecia byłaby z historii biegów, której v1 nie ma —
       * a ta, która stoi na jej miejscu przy niedokończonym szkicu, mówi, czego brakuje, żeby
       * ten workflow dał się w ogóle uruchomić. */}
      <span className="flex gap-3 border-t border-line pt-2 font-mono text-mono text-muted">
        <span>{counted(wf.steps.length, 'step')}</span>
        <span>{counted(differentAgents(wf.steps), 'agent')}</span>
        {waiting === 0 ? null : (
          <span className="text-attend">
            {waiting === 1 ? '1 step has nobody yet' : `${String(waiting)} steps have nobody yet`}
          </span>
        )}
      </span>
    </button>
  );
}
