import type { ReactElement } from 'react';

import type { CellOutcome } from './io';
import { MARKS } from './model';
import type { TableView } from './model';

/* Macierz: wiersz to przypadek, kolumna to wariant.
 *
 * KOMÓRKA NIESIE ZNAK I CENĘ, I ANI JEDNEJ RZECZY WIĘCEJ. Powód nie jest estetyczny: sufit
 * gęstości z `docs/ARCHITECTURE.md` §7 jest mierzony i może tylko maleć, a macierz dziewięć na
 * trzy z pełnym zdaniem w każdej komórce przewraca go natychmiast. Zdanie „dlaczego" stoi pod
 * tabelą, w jednej liście, gdzie czyta się je po kolei — a nie dwadzieścia siedem razy naraz.
 *
 * ŻADNEGO NOWEGO KOLORU (`AGENTS.md` §4). Komórka, która nie przeszła, bierze `fail`; komórka,
 * której nikt nie zmierzył, bierze przygaszony tekst; komórka, która przeszła, nie bierze
 * koloru wcale. Zieleni w tej palecie nie ma i nie ma jej tu brakować: „przeszło" jest stanem
 * spodziewanym, więc to on ma być cichy.
 *
 * ── DWA MARTWE MIEJSCA, NAPRAWIONE 2026-08-31 ──────────────────────────────────────────────
 *
 * WIERSZ BYŁ TEKSTEM BEZ DRZWI. `task`, `expect`, `command` i `proof` leżą w modelu od początku
 * i nie miały ŻADNEJ drogi na ekran: człowiek patrzył na `✗` i nie mógł sprawdzić, czego ta
 * komórka właściwie chciała. Dziś wiersz jest `<details>` — prawdziwą kontrolką, która nie
 * potrzebuje ani jednego handlera, więc jej działanie nie zależy od tego, czy da się je
 * kliknąć w teście. Zwinięta nie kosztuje ani jednego elementu z sufitu gęstości.
 *
 * ZNAK NIE MIAŁ LEGENDY. `·` miał `aria-label="not measured"` i nic poza tym, więc dla OKA był
 * kropką: trzy kropki obok trzech krzyżyków czyta się jako „nic tam nie ma", a znaczą „nikt
 * tego nie zmierzył". To jest ta sama różnica, o którą rozbija się liczba w nagłówku, i dlatego
 * legenda bierze znaki z tej samej tablicy, co komórki — dwie kopie rozjechałyby się po
 * pierwszej zmianie znaku i nikt by tego nie zauważył.
 */

/** Klasa tekstu dla stanu komórki. Jedna tabela, żeby stan i barwa nie rozjechały się w dwóch. */
const TONE: Readonly<Record<CellOutcome, string>> = {
  passed: 'text-ink',
  'did-not-pass': 'text-fail',
  'not-judged': 'text-muted',
};

/** Zdanie, którym czytnik ekranu nazywa stan komórki — i którym legenda nazywa go dla oka. */
const SAID: Readonly<Record<CellOutcome, string>> = {
  passed: 'passed',
  'did-not-pass': 'did not pass',
  'not-judged': 'not measured',
};

/** Kolejność w legendzie: od tego, czego się chce, do tego, czego nikt nie zmierzył. */
const ALL: readonly CellOutcome[] = ['passed', 'did-not-pass', 'not-judged'];

export interface MatrixProps {
  readonly table: TableView;
}

export function Matrix({ table }: MatrixProps): ReactElement {
  return (
    <div>
      <div data-lab-matrix className="paper overflow-auto">
        <table className="w-full border-collapse text-body">
          <thead>
            <tr className="border-b border-line">
              <th scope="col" className="p-2 text-left font-normal label">
                Case
              </th>
              {table.columns.map((column) => (
                <th
                  key={column.id}
                  scope="col"
                  data-lab-column={column.id}
                  className="p-2 text-left text-ui font-normal text-ink"
                >
                  {column.name}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {table.rows.map((row) => (
              <tr key={row.caseId} className="border-b border-line-subtle last:border-b-0">
                <th
                  scope="row"
                  data-lab-row={row.caseId}
                  className="max-w-80 p-2 text-left align-top font-normal text-ink"
                >
                  {row.asks.length === 0 ? (
                    row.name
                  ) : (
                    <details data-lab-case={row.caseId}>
                      <summary className="cursor-pointer">{row.name}</summary>
                      <dl className="mt-2 flex flex-col gap-1">
                        {row.asks.map((one) => (
                          <div key={one.label} className="stack">
                            <dt className="label">{one.label}</dt>
                            <dd className="m-0 text-ui text-body">{one.value}</dd>
                          </div>
                        ))}
                      </dl>
                    </details>
                  )}
                </th>
                {row.cells.map((cell) => (
                  <td
                    key={cell.variantId}
                    data-lab-cell={cell.outcome}
                    /* POWÓD POD KURSOREM, nie w komórce: pełne zdanie w każdej z dwudziestu
                       siedmiu komórek jest ścianą, a sufit gęstości może tylko maleć. Dla
                       czytnika ekranu ten sam powód idzie w nazwę znaku niżej — atrybut `title`
                       bywa dla niego niewidzialny, więc sam nie wystarcza. */
                    title={cell.said === '' ? undefined : cell.said}
                    className={'p-2 align-top ' + TONE[cell.outcome]}
                  >
                    <span
                      aria-label={
                        cell.said === ''
                          ? SAID[cell.outcome]
                          : SAID[cell.outcome] + ' — ' + cell.said
                      }
                    >
                      {cell.mark}
                    </span>
                    {cell.spend === '' ? null : <span className="ml-2 value">{cell.spend}</span>}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="mt-2 flex flex-wrap items-baseline gap-x-4 gap-y-1">
        {/* ZNAKI Z TEJ SAMEJ TABLICY, co komórki. Przepisane tutaj byłyby drugim źródłem
            i pierwsza zmiana znaku zostawiłaby legendę mówiącą o poprzednim. */}
        <p data-lab-legend className="lead">
          {ALL.map((outcome) => MARKS[outcome] + ' ' + SAID[outcome]).join('   ')}
        </p>
        <p data-lab-gesture className="lead">
          Click a case to read what it asks for. Hover a mark to read why it ended that way.
        </p>
      </div>
    </div>
  );
}
