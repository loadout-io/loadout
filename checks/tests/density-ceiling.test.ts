/* AC-4 dla T-22 — sufit gęstości ma JEDNĄ kopię i mieszka w docs/ARCHITECTURE.md §7.
 *
 * Niezmiennik 18 łamie się cicho na dwa sposoby. Pierwszy to zapadka, która przy każdym
 * pomiarze zapisuje aktualną wartość (AC-7). Drugi jest tutaj: siedem liczb przepisanych
 * do `scripts/density-audit.mjs` obok tych w ARCHITECTURE §7. Dwie kopie nie rozjeżdżają się
 * z hukiem — rozjeżdżają się przy pierwszej edycji dokumentu, po której bramka pilnuje
 * liczby, której już nikt nie deklaruje, a człowiek czytający dokument widzi inny limit
 * niż ten, który go zatrzymuje.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(readCeiling(REAL)).toEqual({regions: 8, …})`.
 * Przechodzi ją funkcja zwracająca stały obiekt i w ogóle NIEOTWIERAJĄCA pliku. Rozstrzygają
 * dwie kopie w piaskownicy: w pierwszej podmieniona jest liczba (stała ją przespała),
 * w drugiej usunięty jest cały wiersz (wartość domyślna albo ciche pominięcie metryki
 * przespałoby ją tak samo, a to jest gorszy błąd — metryka, której nikt nie mierzy,
 * czyta się dokładnie jak metryka, która się mieści).
 *
 * ETYKIETY WIERSZY są częścią kontraktu i dlatego są tu wpisane wprost. To NIE jest druga
 * kopia sufitu: kopiowane są nazwy, nie limity. Bez nazw parser nie ma jak stwierdzić, że
 * wiersza brakuje — mógłby tylko zwrócić sześć wpisów i zamilknąć. Przemianowanie wiersza
 * w dokumencie ma zapalić to kryterium na czerwono, a nie po cichu zgubić metrykę.
 *
 * ─────────────────────────────────────────────────────────────────────────────────────────
 * ROZJAZD DO ZGŁOSZENIA CZŁOWIEKOWI (AGENTS.md §7), świadomy i opisany:
 *
 * TASK.md wymienia siedem wartości jako `8, 96, 60, 1, 4, 2, 1`. Żywy `docs/ARCHITECTURE.md`
 * §7 deklaruje w siódmym wierszu `| Osie nawigacji na ekranie | **2**, i muszą być
 * prostopadłe |`, a proza tuż pod tabelą opisuje tę zmianę wprost: "Pierwotnie limit brzmiał
 * »jedna metafora« … Karty go zmieniają, ale nie łamią: warunkiem jest prostopadłość".
 * Wiersz zmienił i nazwę, i wartość; TASK.md cytuje wersję sprzed tej zmiany.
 *
 * Pinujemy DOKUMENT, nie TASK.md, z trzech powodów. AGENTS.md §3 wygrywa z plikiem zadania,
 * a niezmiennik 18 mówi "sufit z docs/ARCHITECTURE.md §7". Samo TASK.md nazywa §7 "jedynym
 * źródłem siedmiu liczb — skrypt je stąd parsuje, nie kopiuje". A lista w TASK.md jest
 * dokładnie tą drugą kopią, przed którą to kryterium ma bronić: dokument został poprawiony,
 * kopia nie. Wpisanie tutaj `1` dałoby kryterium, którego nie da się spełnić NIGDY — parser
 * czytający prawdziwy plik zwróci 2, a `docs/` nie jest w bloku OWNS tego zadania.
 * ─────────────────────────────────────────────────────────────────────────────────────────
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { readCeiling } from '../../scripts/density-audit.mjs';
import { mustExist, plant, sandbox } from './_support';

const DOC = 'docs/ARCHITECTURE.md';

/** Etykiety wierszy tabeli §7, w kolejności wierszy. */
const LABELS = [
  'Oznaczone regiony na ekranie',
  'Piksele chrome nad pierwszą treścią',
  'Elementy niosące tekst w widoku domyślnym',
  'Żywe regiony na jeden fakt',
  'Linie tekstu w kafelku agenta',
  'Regiony animujące się od jednego zdarzenia',
  'Osie nawigacji na ekranie',
];

/** Klucze maszynowe — tym mówi zrzut kolektora i tym mówi sędzia (AC-5, AC-6). */
const KEYS = [
  'labelledRegions',
  'chromePixels',
  'textElements',
  'liveRegionsPerFact',
  'agentCardLines',
  'animatedRegions',
  'navigationAxes',
];

const REAL = mustExist(DOC, 'the only source of the seven density numbers');
const SOURCE = readFileSync(REAL, 'utf8');

/** Kopia dokumentu z jedną zmianą. Rzuca, kiedy zmiana była pusta — inaczej test przechodzi na nic. */
function copyWith(name: string, edit: (text: string) => string): string {
  const edited = edit(SOURCE);
  if (edited === SOURCE) {
    throw new Error(
      `the fixture edit "${name}" changed nothing in ${DOC}, so it proves nothing about the parser`,
    );
  }
  const dir = sandbox(`ceiling-${name}`);
  plant(dir, DOC, edited);
  return join(dir, DOC);
}

describe('readCeiling parses the ceiling out of the document that declares it', () => {
  it('returns the seven rows of §7, in table order, with the limits the document states', () => {
    const ceiling = readCeiling(REAL);

    expect(ceiling.map((entry) => entry.label)).toEqual(LABELS);
    expect(ceiling.map((entry) => entry.key)).toEqual(KEYS);
    expect(ceiling.map((entry) => entry.limit)).toEqual([8, 96, 60, 1, 4, 2, 2]);
  });

  it('follows the document when a limit changes, because it holds no copy of its own', () => {
    const path = copyWith('nine', (text) =>
      text.replace('| Oznaczone regiony na ekranie | **8** |', '| Oznaczone regiony na ekranie | **9** |'),
    );

    const ceiling = readCeiling(path);

    // Stała wpisana w .mjs zwróciłaby tu 8 i nikt by się nie dowiedział, że dokument mówi co innego.
    expect(ceiling[0]?.limit).toBe(9);
    expect(ceiling[0]?.label).toBe(LABELS[0]);
    expect(ceiling).toHaveLength(7);
  });

  it('refuses by name when a row is gone, instead of defaulting or dropping the metric', () => {
    const path = copyWith('missing-row', (text) =>
      text
        .split('\n')
        .filter((line) => !line.startsWith('| Elementy niosące tekst w widoku domyślnym |'))
        .join('\n'),
    );

    // Wartość domyślna byłaby najgorszym z możliwych wyjść: metryka, której nikt nie mierzy,
    // czytałaby się jak metryka, która się mieści (to jest cała treść AC-6).
    expect(() => readCeiling(path)).toThrow(/Elementy niosące tekst w widoku domyślnym/);
  });
});
