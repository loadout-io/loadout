/* Okno historii w modelu widoku — sprawdzenie DOPISANE do kryteriów, nie zamiast żadnego.
 *
 * Kryterium 6 mierzy okno w magazynie (`src/state/run.ts`): tam mieszkają linie. Ale model
 * widoku trzyma DRUGĄ strukturę o tym samym suficie — wiersze — i nikt jej nie mierzy, mimo że
 * to ona rośnie przez cały bieg i to ona przycina się z przesuwaniem indeksów otwartych grup
 * sklejania. Błąd w tej arytmetyce jest cichy dokładnie tak, jak lubi być cichy: pierwsze dwa
 * tysiące wierszy wygląda bez zarzutu, a rozjazd zaczyna się w połowie długiego biegu, kiedy
 * nikt już nie patrzy na wiersz numer 1.
 *
 * Trzy rzeczy tutaj, każda o inną awarię:
 *   - okno trzyma NAJNOWSZE wiersze, nie najstarsze (to samo, przed czym broni kryterium 6,
 *     tyle że po drugiej stronie granicy model↔magazyn),
 *   - wiersz, który przeżył przycięcie, jest TYM SAMYM obiektem — przemapowanie całej historii
 *     przy przycinaniu jest poprawne co do wartości i kosztuje React tyle samo, co przy każdej
 *     paczce,
 *   - grupa sklejania otwarta PRZED przycięciem nie dolicza się do cudzego wiersza. To jest
 *     najgorszy możliwy skutek pomyłki o `shift`: licznik rośnie na wierszu, który należy do
 *     kogoś innego, i wygląda dokładnie jak wynik poprawny.
 */
import { describe, expect, it } from 'vitest';
import { LINE_LIMIT } from '../../../state/run';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

const AGENT = 'Forge';
const OTHER = 'Needle';
const OVERFLOW = 500;

/** Odstęp większy niż okno sklejania: te wiersze mają się NIE sklejać. */
const APART_MS = 3_000;

describe('the history window in the view model', () => {
  it('keeps the newest rows and lets go of the oldest', () => {
    const feed = createFeed(sealedScroller());
    const total = LINE_LIMIT + OVERFLOW;
    for (let i = 1; i <= total; i += 1) {
      feed.appendLines([line.note(i, i * APART_MS, AGENT, 'Row ' + String(i) + '.')]);
    }

    const { history } = feed.view;
    expect(history.length, 'the window is as wide as the one the store keeps on lines').toBe(
      LINE_LIMIT,
    );
    expect(
      history.at(-1)?.id,
      'the newest row survives. Trimming the tail keeps the oldest rows, the length still ' +
        'reads two thousand, and the view looks like a run that stopped',
    ).toBe(total);
    expect(history[0]?.id, 'and the window starts right after what fell out of the head').toBe(
      OVERFLOW + 1,
    );
  });

  it('hands the same row objects back after the window slid past them', () => {
    const feed = createFeed(sealedScroller());
    for (let i = 1; i <= LINE_LIMIT; i += 1) {
      feed.appendLines([line.note(i, i * APART_MS, AGENT, 'Row ' + String(i) + '.')]);
    }
    const survivor = feed.view.history.find((row) => row.id === LINE_LIMIT);

    for (let i = LINE_LIMIT + 1; i <= LINE_LIMIT + OVERFLOW; i += 1) {
      feed.appendLines([line.note(i, i * APART_MS, AGENT, 'Row ' + String(i) + '.')]);
    }

    expect(
      feed.view.history.find((row) => row.id === LINE_LIMIT),
      'the row that stayed in the window IS the object that was there before — trimming moves ' +
        'the window, it does not rebuild the rows inside it',
    ).toBe(survivor);
  });

  it('never folds a line into a row that the trim has already moved', () => {
    const feed = createFeed(sealedScroller());
    /* Forge otwiera grupę odczytów, a potem Needle zalewa okno prozą tak, że wiersz Forge'a
     * wypada z głowy. Zalew MUSI być od drugiego agenta: linia tego samego agenta zamknęłaby
     * grupę sama z siebie i przypadek nigdy nie doszedłby do arytmetyki przycinania. */
    feed.appendLines([line.read(1, 0, AGENT, 'src/parser.rs')]);
    feed.appendLines(
      Array.from({ length: LINE_LIMIT + OVERFLOW }, (_, i) =>
        line.note(i + 2, (i + 2) * APART_MS, OTHER, 'Row ' + String(i + 2) + '.'),
      ),
    );

    /* Drugi odczyt Forge'a WEWNĄTRZ okna dwóch sekund tamtej grupy — czyli dokładnie wtedy,
     * kiedy nieprzesunięty indeks jeszcze pasuje i wskazuje już na cudzy wiersz. */
    feed.appendLines([line.read(90_001, 1_000, AGENT, 'src/main.rs')]);

    expect(
      feed.view.history.filter((row) => row.count > 1),
      'the group Forge opened left the window, so his second read opens its own row. A count ' +
        'that lands on a row the trim moved underneath it hands one agent the work another one ' +
        'did — the same false attribution the folding criterion catches, only later in the run ' +
        'and with nobody watching',
    ).toEqual([]);
    expect(
      feed.view.history.filter((row) => row.agent === OTHER && row.kind !== 'note').length,
      'and no row of the agent who only wrote prose is wearing a read',
    ).toBe(0);
    expect(feed.view.history.at(-1)?.agent, 'the new read is its own row, at the end').toBe(AGENT);
  });
});
