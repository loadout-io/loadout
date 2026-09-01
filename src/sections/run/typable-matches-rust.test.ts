/* Nazwa workflow do wpisania znaczy to samo po obu stronach granicy.
 *
 * # Po co to istnieje
 *
 * Ta sama reguła musi żyć dwa razy i nie da się tego uniknąć: TEN wiersz normalizuje to, co
 * człowiek NAPISAŁ (czyli potrzebuje funkcji, nie wartości), a rustowy `commands::workflows::
 * typable` oddaje liderowi nazwy, którymi ma się posłużyć w `list_workflows`. Dwie implementacje
 * jednej reguły rozjeżdżają się cicho, a skutek nie wygląda jak błąd: lider proponuje nazwę,
 * Enter odpowiada „There is no workflow called…", i człowiek widzi workflow, którego rzekomo
 * nie ma.
 *
 * # Dlaczego wspólna fikstura, a nie przepisane pary
 *
 * Bo pary wpisane po obu stronach z palca są zielone także wtedy, gdy obie strony mylą się tak
 * samo — a przy dwóch niezależnych implementacjach to jest najczęstszy sposób, w jaki takie
 * kryterium kłamie (niezmiennik 20). Plik jest jeden i czyta go także
 * `src-tauri/tests/it/typable_names_match_the_window.rs`.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { typable } from './run-command';

interface Pair {
  readonly given: string;
  readonly typable: string;
}

/* Ścieżka od korzenia repo, bo stamtąd biegnie vitest. Odczyt jest tu celowo goły: plik, którego
 * nie ma, ma wywrócić kryterium, a nie oddać pustą listę par i przejść na zero iteracji. */
const FIXTURE = JSON.parse(readFileSync('docs/patterns/fixtures/typable-names.json', 'utf8')) as {
  readonly pairs: readonly Pair[];
};

describe('the name a person types means the same on both sides', () => {
  it('carries enough pairs to be an oracle at all', () => {
    expect(
      FIXTURE.pairs.length,
      'a fixture that shrank to a handful of pairs stops being an oracle and starts being a ' +
        'formality',
    ).toBeGreaterThanOrEqual(10);
  });

  it('agrees with the shared fixture on every pair', () => {
    const wrong = FIXTURE.pairs
      .filter((pair) => typable(pair.given) !== pair.typable)
      .map((pair) => `${JSON.stringify(pair.given)} -> ${JSON.stringify(typable(pair.given))}`);

    expect(
      wrong,
      'these names mean one thing here and another in Rust, so the lead would hand the person a ' +
        'name that Enter refuses: ' +
        wrong.join(', '),
    ).toEqual([]);
  });
});
