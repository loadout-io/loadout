import type { ReactElement } from 'react';

import type { CellOutcome } from './io';
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
 */

/** Klasa tekstu dla stanu komórki. Jedna tabela, żeby stan i barwa nie rozjechały się w dwóch. */
const TONE: Readonly<Record<CellOutcome, string>> = {
  passed: 'text-ink',
  'did-not-pass': 'text-fail',
  'not-judged': 'text-muted',
};

/** Zdanie, którym czytnik ekranu nazywa stan komórki. */
const SAID: Readonly<Record<CellOutcome, string>> = {
  passed: 'passed',
  'did-not-pass': 'did not pass',
  'not-judged': 'not measured',
};

export interface MatrixProps {
  readonly table: TableView;
}

export function Matrix({ table }: MatrixProps): ReactElement {
  return (
    <div data-lab-matrix className="overflow-auto rounded-md border border-line bg-panel">
      <table className="w-full border-collapse text-body">
        <thead>
          <tr className="border-b border-line">
            <th scope="col" className="p-2 text-left text-ui font-normal text-muted">
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
                className="max-w-80 p-2 text-left font-normal text-ink"
              >
                {row.name}
              </th>
              {row.cells.map((cell) => (
                <td
                  key={cell.variantId}
                  data-lab-cell={cell.outcome}
                  className={'p-2 ' + TONE[cell.outcome]}
                >
                  <span aria-label={SAID[cell.outcome]}>{cell.mark}</span>
                  {cell.spend === '' ? null : (
                    <span className="ml-2 text-ui text-muted">{cell.spend}</span>
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
