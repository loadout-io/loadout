/* Zejście biegu gasi KAŻDE pole, które opisywało żywy bieg — a lista tych pól jest tu wypisana.
 *
 * CZWARTY RAZ TEN SAM KSZTAŁT. T-66 zdjął widmowy kafelek z listy agentów, T-67 widmowy wiersz
 * ze strefy TERAZ, i za każdym razem przyczyna była jedna: model trzyma kilka pól opisujących
 * ŻYWY bieg, a koniec biegu gasi tylko niektóre. Zmierzone dziś: `runEnded` gasi `doing`,
 * `thinking`, `parked` i `toCarry` — a `waiting` zostaje. Więc pytanie, na które człowiek nie
 * odpowiedział przed Stopem albo przed błędem, przeżywa bieg, który je zadał: `pinned` stoi,
 * `attention` stoi na `you`, a przyciski odpowiedzi dalej wołają `answer()` dla agenta, który
 * nie pracuje. Kontrolka bez roboty (niezmiennik 16) przypięta do relacji, której w danych już
 * nie ma (niezmiennik 17).
 *
 * DLACZEGO LISTA JEST WYPISANA, A NIE POLICZONA. Suma („po biegu jest pusto") przechodzi dla
 * implementacji, która gasi jedno pole i zostawia cztery — czyli dla dokładnie tego stanu, z
 * którego wzięły się trzy poprzednie zadania. Tabela `LIVE` niżej wypisuje pole po polu, a jej
 * korzenie są porównywane z KLUCZAMI widoku, więc kolejne pole strefy żywej dopisane do modelu
 * zapala to kryterium, zanim ktoś napisze czwarty przypis: albo nie ma go na żadnej z dwóch list
 * (pęka podział), albo jest na tej pierwszej i nie gaśnie (pęka wygaszenie). Ten sam kształt, co
 * dwie wypisane listy rodzajów w `./collapse.test.ts` — „dziewięć złych to nadal dziewięć".
 *
 * SŁABA WERSJA: `expect(view.pinned).toBeNull()` po `runEnded()`. Mówi o jednym polu z sześciu
 * i nie ma nic do powiedzenia o siódmym.
 *
 * CICHA PORAŻKA, PRZED KTÓRĄ STOJĄ DWA OSTATNIE PRZYPADKI: naprawa przez zbudowanie modelu od
 * nowa. Wyczyściłaby wszystko naraz i skasowała HISTORIĘ, czyli transkrypt biegu, który właśnie
 * zszedł — a to jest jedyna rzecz, po którą człowiek na ten ekran wraca.
 */
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import type { FeedView } from './model';
import { createFeed } from './model';

const FORGE = 'Forge';
const NEEDLE = 'Needle';

const FORGE_SAID = 'Rewriting the splitter.';
const NEEDLE_SAID = 'Checking the header row.';
const FORGE_RESUMED = 'Back on the splitter.';

const FIRST_QUESTION = 'Should the old splitter stay behind a switch?';
const SECOND_QUESTION = 'Which header row is the real one?';

/** Zdanie człowieka na pierwsze pytanie — jednocześnie opcja, którą to pytanie podało. */
const ANSWERED = 'Yes, keep the old one behind a switch.';

/** Numer pierwszego pytania. Odpowiedź zdejmuje DOKŁADNIE je; drugie zostaje przypięte. */
const FIRST_ID = 3;

/**
 * Wiersze historii, które ta scena zostawia, po numerach.
 *
 * Pięć, nie sześć: `thinking` jest statusem i do historii nie wchodzi [T2 §7.3 reguła 5].
 * Wypisane wprost, bo porównanie listy z samą sobą przeszłoby też na historii pustej — a wtedy
 * kontrola przeciw przebudowie modelu nie kontrolowałaby niczego.
 */
const HISTORY_IDS: readonly number[] = [1, 2, FIRST_ID, 4, 5];

/** Jedno pole widoku, które opisuje ŻYWY bieg. */
interface LiveField {
  /** Pole `FeedView`, w którym to siedzi. Korzeń, bo z korzeni powstaje podział kluczy niżej. */
  readonly field: keyof FeedView;
  /** Jak się do niego dochodzi — nazwa dla człowieka, który czyta czerwień. */
  readonly at: string;
  /** Co to pole mówi, w postaci, którą da się porównać. */
  readonly reads: (view: FeedView) => unknown;
  /** Jak to samo pole czyta się wtedy, kiedy nic nie żyje. */
  readonly quiet: unknown;
  /** Co zostaje na ekranie, kiedy to pole przeżyje bieg. Wchodzi do komunikatu czerwieni. */
  readonly says: string;
}

/**
 * Zamknięta lista pól strefy żywej. WYPISANA, nigdy policzona.
 *
 * Kolejność jest kolejnością z prozy zadania (linia agenta, myślenie, pytanie, czyja kolej,
 * stanie na punkcie kontrolnym, zdanie do przewiezienia), a nie kolejnością z pliku modelu:
 * tak czyta się ją razem ze zdaniem, które te pola razem opowiadają.
 */
const LIVE: readonly LiveField[] = [
  {
    field: 'now',
    at: 'now.rows',
    reads: (view) => view.now.rows.map((row) => row.agent),
    quiet: [],
    says: 'agents standing in the zone that says what is happening, over work nobody is doing',
  },
  {
    field: 'now',
    at: 'now.thinking',
    reads: (view) => view.now.thinking,
    quiet: null,
    says: 'the Thinking… slot alive for a run that is gone',
  },
  {
    field: 'pinned',
    at: 'pinned',
    reads: (view) => view.pinned?.text ?? null,
    quiet: null,
    says: 'a question card left standing, with answer buttons that reach an agent who stopped',
  },
  {
    field: 'attention',
    at: 'attention',
    reads: (view) => view.attention,
    quiet: 'agents',
    says: 'the screen saying it is your turn when nobody is waiting on you',
  },
  {
    field: 'parked',
    at: 'parked',
    reads: (view) => view.parked,
    quiet: false,
    says: 'the control that lets a run through, left over a run there is nothing to let through',
  },
  {
    field: 'toCarry',
    at: 'toCarry',
    reads: (view) => view.toCarry,
    quiet: '',
    says: 'a sentence queued for delivery to an agent who is no longer listening',
  },
];

/**
 * Pola, które koniec biegu ZOSTAWIA — i to jest druga połowa podziału.
 *
 * Oba są zapisem tego, co się stało, nie stanem tego, co żyje: historia jest transkryptem,
 * a `answers` zapisem tego, co człowiek odpowiedział. Gaszenie ich razem ze strefą żywą jest
 * dokładnie tą cichą porażką, którą opisuje nagłówek.
 */
const KEPT: readonly (keyof FeedView)[] = ['history', 'answers'];

/** Każde pole widoku należy do dokładnie jednej z dwóch list wyżej. */
const CLASSIFIED: readonly string[] = [
  ...new Set<string>([...LIVE.map((entry) => entry.field), ...KEPT]),
];

/**
 * Czy to pole czyta się dokładnie tak, jak czyta się przy niczym niebiegnącym.
 *
 * Przez `JSON.stringify`, bo wartości w tabeli są tablicami napisów, napisami, wartościami
 * logicznymi i `null`. Powód jest jeden i mierzalny: pozwala zebrać WSZYSTKIE pola, które
 * przeżyły, w jedną asercję. Pętla z `expect` w środku pęka na pierwszym z nich i mówi o jednym
 * polu z sześciu — czyli mówi dokładnie tyle, ile mówiła słaba wersja tego kryterium.
 */
function isQuiet(entry: LiveField, view: FeedView): boolean {
  return JSON.stringify(entry.reads(view)) === JSON.stringify(entry.quiet);
}

/** Które pola strefy żywej stoją w tym widoku zapalone. */
function lit(view: FeedView): readonly string[] {
  return LIVE.filter((entry) => !isQuiet(entry, view)).map((entry) => entry.at);
}

/** Co dokładnie mówi każde pole, które przeżyło — jedno zdanie na pole. */
function complaints(view: FeedView): readonly string[] {
  return LIVE.filter((entry) => !isQuiet(entry, view)).map((entry) => entry.at + ': ' + entry.says);
}

/**
 * Bieg, w którym ŻYJE WSZYSTKO NARAZ.
 *
 * DWA PYTANIA, NIE JEDNO, i to jest jedyny sposób, żeby `toCarry` i `pinned` stały zapalone
 * w tej samej chwili: odpowiedź zdejmuje przypięcie dokładnie tego pytania, na które padła, więc
 * scena z jednym pytaniem gasi jedno z dwóch pól, zanim cokolwiek zostanie zmierzone. Kolejka
 * pytań istnieje w modelu właśnie dlatego, że bieg stoi na najstarszym nieodpowiedzianym.
 *
 * Zdanie od agenta na końcu, a myślenie za nim: prawdziwa linia gasi slot, więc odwrotna
 * kolejność zostawiłaby `now.thinking` puste i scena przestałaby zapalać wszystko.
 */
function everythingLive() {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, FORGE, FORGE_SAID), line.note(2, 500, NEEDLE, NEEDLE_SAID)]);
  feed.appendLines([
    line.asked(FIRST_ID, 1_000, FORGE, FIRST_QUESTION, [ANSWERED]),
    line.asked(4, 1_500, NEEDLE, SECOND_QUESTION, []),
  ]);
  feed.answer(FIRST_ID, ANSWERED);
  feed.appendLines([line.note(5, 2_000, FORGE, FORGE_RESUMED), line.thinking(6, 2_500, NEEDLE)]);
  return feed;
}

describe('nothing that described a live run survives the run', () => {
  it('lights every field on that list while the run is going', () => {
    const feed = everythingLive();

    expect(
      LIVE.filter((entry) => isQuiet(entry, feed.view)).map((entry) => entry.at),
      'these fields were already empty while the run was going, so the emptiness asked for ' +
        'below proves nothing about them: an implementation that puts out nothing at all would ' +
        'pass on the empty sum. The scene has to light every one of them at once.',
    ).toEqual([]);
  });

  it('puts out every field on that list when the run goes down', () => {
    const feed = everythingLive();

    feed.runEnded();

    expect(
      complaints(feed.view),
      'a run that is gone left these behind, and each one reads on screen exactly like work in ' +
        'flight. Three tasks in this family have already been spent on one field each; the list ' +
        'is written out here so the fix has to be the whole list at once, not the field somebody ' +
        'noticed this week.',
    ).toEqual([]);
  });

  it('names every field of the view, so a new one cannot arrive unclassified', () => {
    const feed = everythingLive();

    expect(
      Object.keys(feed.view).sort(),
      'the view carries a field that neither list names. That is how this defect has arrived ' +
        'four times: somebody adds a field that describes a LIVE run, the end of a run never ' +
        'learns about it, and every emptiness check here keeps passing because it never asked ' +
        'about that field. Put the new name on the live list (and then put it out) or on the ' +
        'kept list (and then say in a comment why the end of a run must not touch it).',
    ).toEqual([...CLASSIFIED].sort());
  });

  it('leaves the whole transcript standing, because that is what a person comes back for', () => {
    const feed = everythingLive();
    const before = feed.view.history;

    feed.runEnded();

    expect(
      feed.view.history.map((row) => row.label),
      'the end of a run clears the fields that say what is happening, never the account of what ' +
        'happened. A person comes back to this screen to read the run that just went down.',
    ).toEqual([FORGE_SAID, NEEDLE_SAID, FIRST_QUESTION, SECOND_QUESTION, FORGE_RESUMED]);
    expect(
      feed.view.history,
      'and it is the SAME array: a fresh one asks React to redraw the whole transcript for ' +
        'something that never entered it',
    ).toBe(before);
    expect(
      feed.view.answers,
      'what the person answered is an account too, and it is kept forever. Emptying it with the ' +
        'live fields is the quiet failure this file was written against.',
    ).toEqual([{ questionId: FIRST_ID, option: ANSWERED, who: 'you' }]);
  });

  it('keeps the row identifiers, so nobody repairs this by building the model again', () => {
    const feed = everythingLive();

    expect(
      feed.view.history.map((row) => row.id),
      'the scene has to leave five rows behind before the run goes down, or the comparison after ' +
        'it is a comparison of two empty lists',
    ).toEqual(HISTORY_IDS);

    feed.runEnded();

    expect(
      feed.view.history.map((row) => row.id),
      'building the feed again empties every live field in one line and takes the transcript ' +
        'with it. The rows have to be the very same rows: same identifiers, same order, nothing ' +
        'renumbered underneath the screen.',
    ).toEqual(HISTORY_IDS);
  });

  it('fills the live fields again on the next run', () => {
    const feed = everythingLive();
    feed.runEnded();

    feed.appendLines([line.note(7, 10_000, FORGE, FORGE_RESUMED)]);
    feed.appendLines([line.asked(8, 10_500, FORGE, SECOND_QUESTION, [ANSWERED])]);

    expect(
      lit(feed.view),
      'a run that is really going has to light these fields again. Switching them off for good ' +
        'passes every emptiness check above and deletes the zone for the runs that are alive — ' +
        'which is a worse screen than the one this file exists to fix.',
    ).toEqual(['now.rows', 'pinned', 'attention', 'parked']);
  });
});
