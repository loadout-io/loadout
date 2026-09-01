/* Koniec biegu zamyka otwarte okna sklejania: pierwsza linia NOWEGO biegu dostaje własny wiersz.
 *
 * PIĄTY RAZ TEN SAM KSZTAŁT, i to jest jedyny powód, dla którego ten plik istnieje osobno.
 * T-66 zdjął widmowy kafelek z szyny agentów, T-67 widmowy wiersz ze strefy TERAZ, T-68 pola
 * widoku razem z przypiętym pytaniem. Za każdym razem przyczyna była jedna: model trzyma stan
 * opisujący ŻYWY bieg, a koniec biegu gasi tylko część. Mapa otwartych grup sklejania (`groups`
 * w `./model.ts`) jest tym stanem o warstwę niżej — siedzi w domknięciu `createFeed`, nie
 * w `FeedView`, więc kryterium chodzące po `Object.keys(view)` nie ma o nią jak zapytać.
 *
 * DLACZEGO TO BOLI DOPIERO TUTAJ. `feedFor()` oddaje JEDEN model na zakres i trzyma go do końca
 * życia okna (rejestr w `./live.ts` nie ma usuwania i to jest wymóg, nie przeoczenie). Model
 * przeżywa więc bieg z zapasem. Grupa otwarta ostatnią sklejalną linią biegu, który właśnie
 * zszedł, zostaje otwarta — a pierwsza linia następnego biegu, jeśli trafi w okno 2 s liczone
 * od TAMTEJ linii, dolicza się do TAMTEGO wiersza. Jedno „Read 2 files" rozpięte na dwóch
 * biegach, z jedną listą identyfikatorów i jednym rozwinięciem: relacja, której w danych nie ma
 * (niezmiennik 17), i jeden licznik kłamiący o obu biegach naraz (niezmiennik 13).
 *
 * DLACZEGO KRYTERIUM JEST BEHAWIORALNE, A NIE STRUKTURALNE. Nie pyta, czy mapa jest pusta —
 * pyta, czy nowy bieg dostaje własny wiersz. Asercja o zawartości prywatnego stanu wymagałaby
 * albo wystawienia go na zewnątrz (drugie miejsce prawdy o tym samym, niezmiennik 13), albo
 * testu, który zna wnętrze modułu i pęka przy każdej zmianie implementacji, także poprawnej.
 *
 * CICHA PORAŻKA, PRZED KTÓRĄ STOI TEN PLIK: naprawa przez skasowanie sklejania. Grupy istnieją,
 * bo bez nich sześć odczytów w jednej sekundzie daje sześć wierszy i ścianę tekstu, którą cała
 * teza DESIGN §1 istnieje żeby skasować. Stoją przeciw temu DWA przypadki, nie jeden: sklejanie
 * ma działać przed zejściem biegu (para bez `runEnded()` między liniami) i po nim (para w środku
 * NOWEGO biegu). Wersja gasząca sklejanie na zawsze przechodzi pierwszy z nich i pęka na drugim.
 *
 * SŁABA WERSJA: `expect(view.history).toHaveLength(2)` i nic poza tym. Przechodzi dla
 * implementacji, która sklejania w ogóle nie ma, a przy dwóch liniach na scenie nie ma jak tego
 * pokazać. Przechodzi też dla dwóch znaczników czasu położonych tak daleko od siebie, że nie
 * skleiłyby się nigdy — dlatego ta sama para znaczników jedzie przez wszystkie cztery przypadki.
 */
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

const FORGE = 'Forge';

/* Proza sprzed sklejalnej linii. Jest tu po to, żeby „historia została nietknięta" miało co
 * mierzyć: porównanie jednoelementowej listy z jednoelementową przechodzi także wtedy, kiedy
 * zejście biegu przepisało wiersz, którego dotyczy. */
const EARLIER = 'Rewriting the field splitter.';

const FIRST_FILE = 'src/splitter.ts';
const SECOND_FILE = 'src/header.ts';
const THIRD_FILE = 'src/quotes.ts';

/* PARA ZNACZNIKÓW, NA KTÓREJ STOI CAŁY PLIK — 500 ms od siebie, czyli głęboko w oknie sklejania
 * (2 s, liczone od PIERWSZEJ linii grupy). Jedna para przez wszystkie przypadki, bo to jest cała
 * kontrola przeciw pustemu przejściu: „dwa wiersze" jest wynikiem dopiero wtedy, kiedy ta sama
 * para bez zejścia biegu między liniami daje wiersz JEDEN. */
const OPENS = { id: 2, at: 1_000 } as const;
const INSIDE = { id: 3, at: 1_500 } as const;

/** Etykiety, które ta scena zostawia przed zejściem biegu. Wypisane, nie policzone ze sceny. */
const BEFORE_THE_END: readonly string[] = [EARLIER, 'Read 1 file'];

/**
 * Bieg do chwili tuż przed zejściem: proza, a po niej jedna sklejalna linia.
 *
 * Odczyt na końcu, nie w środku: to on zostawia OTWARTĄ grupę, czyli dokładnie ten stan, który
 * ma nie przeżyć biegu. Scena kończąca się prozą zamknęłaby grupę sama z siebie i mierzyłaby
 * potem zachowanie, które zachodzi bez żadnej poprawki.
 */
function upToTheEndOfTheRun() {
  const feed = createFeed(sealedScroller());
  feed.appendLines([
    line.note(1, 0, FORGE, EARLIER),
    line.read(OPENS.id, OPENS.at, FORGE, FIRST_FILE),
  ]);
  return feed;
}

describe('the window that folds neighbouring lines does not reach across the end of a run', () => {
  it('gives the next run its own row instead of growing the last row of the one before', () => {
    const feed = upToTheEndOfTheRun();

    expect(
      feed.view.history.map((row) => row.label),
      'the run has to leave an OPEN fold window behind before it goes down, or everything ' +
        'below is a statement about a scene that never had the defect. Two rows: the prose, ' +
        'and one read holding the window open.',
    ).toEqual(BEFORE_THE_END);

    feed.runEnded();
    feed.appendLines([line.read(INSIDE.id, INSIDE.at, FORGE, SECOND_FILE)]);

    expect(
      feed.view.history.map((row) => row.ids),
      'the first foldable line of the NEXT run grew the LAST row of the run before it. One row ' +
        'now stands over two runs, with one list of identifiers behind it and one place to ' +
        'expand — a relationship the data does not have (invariant 17). Three rows here: the ' +
        'prose, the read from the run that went down, and the read from the run that followed.',
    ).toEqual([[1], [OPENS.id], [INSIDE.id]]);
    expect(
      feed.view.history.at(-1)?.count,
      'and the new row counts from itself. A row that came out of the previous run carries its ' +
        'count forward, so the number on screen is a lie about both runs (invariant 13).',
    ).toBe(1);
    expect(
      feed.view.history.at(-1)?.label,
      'the count is always inside the label, so this is where the lie is legible: a run that ' +
        'read one file has to say "Read 1 file", never "Read 2 files" because the run before it ' +
        'also read one',
    ).toBe('Read 1 file');
  });

  it('leaves the transcript standing, because closing a window is not deleting a line', () => {
    const feed = upToTheEndOfTheRun();
    const before = feed.view.history;

    feed.runEnded();

    expect(
      feed.view.history.map((row) => row.label),
      'the end of a run closes the windows that were still open; it never touches the account ' +
        'of what happened. A person comes back to this screen to read the run that just went ' +
        'down, and the fix that empties the rows along with the windows takes that away.',
    ).toEqual(BEFORE_THE_END);
    expect(
      feed.view.history,
      'and it is the very same array. A fresh one asks React to redraw the whole transcript ' +
        'for something that never entered it — and building the model again empties every ' +
        'window in one line while doing exactly that.',
    ).toBe(before);
  });

  it('still folds two neighbouring reads when no run went down between them', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([
      line.note(1, 0, FORGE, EARLIER),
      line.read(OPENS.id, OPENS.at, FORGE, FIRST_FILE),
      line.read(INSIDE.id, INSIDE.at, FORGE, SECOND_FILE),
    ]);

    expect(
      feed.view.history.map((row) => row.ids),
      'THE SAME PAIR OF STAMPS as the case above, with nothing between them. Two things fall ' +
        'out of one assertion here. First: those two lines really do sit inside the fold ' +
        'window, so the two rows measured above are the end of the run doing the work and not ' +
        'two stamps that were never going to fold. Second: folding still exists. Repairing ' +
        'this by switching folding off passes every "two rows" above and turns six reads in ' +
        'one second back into six rows and a wall of text (DESIGN §1).',
    ).toEqual([[1], [OPENS.id, INSIDE.id]]);
    expect(feed.view.history.at(-1)?.label, 'and the folded row counts both of them').toBe(
      'Read 2 files',
    );
  });

  it('folds again inside the run that follows, so the windows are closed and not switched off', () => {
    const feed = upToTheEndOfTheRun();

    feed.runEnded();
    feed.appendLines([
      line.read(INSIDE.id, INSIDE.at, FORGE, SECOND_FILE),
      line.read(4, INSIDE.at + 500, FORGE, THIRD_FILE),
    ]);

    expect(
      feed.view.history.map((row) => row.ids),
      'the run that went down closed its own windows and left the next run without any. Two ' +
        'neighbouring reads of the new run have to fold like any other pair — an implementation ' +
        'that stops folding after the first end of a run passes every other case in this file ' +
        'and gives the next run the wall of text folding exists to prevent.',
    ).toEqual([[1], [OPENS.id], [INSIDE.id, 4]]);
    expect(
      feed.view.history.at(-1)?.label,
      'and the new run counts its own two reads, from one',
    ).toBe('Read 2 files');
  });
});
