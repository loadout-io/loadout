/* Kryterium 3: zwinięte domyślnie; błąd rozwija się sam i nic poza nim.
 *
 * `expect(isExpanded(failed)).toBe(true)` przechodzi dla implementacji, która po pierwszym
 * błędzie rozwija CAŁY strumień — „tryb paniki" — a to jest dokładnie ta ściana tekstu, przed
 * którą stoi cała reguła 2. Rozróżniają to dwie rzeczy:
 *
 *   - zbiór rozwiniętych identyfikatorów po błędzie różni się od zbioru sprzed błędu o DOKŁADNIE
 *     jeden element. Sąsiedni `ran` z `ok: true` i sąsiedni `edit` mają zostać zwinięte,
 *   - pierwsza pokazana linia wyjścia to numer 21 z czterdziestu, nie numer 1. Pomyłka
 *     `slice(0, 20)` zamiast `slice(-20)` pokazuje początek logu, czyli tę jego połowę, która
 *     nigdy nie zawiera powodu — i wygląda identycznie w każdej asercji na samą długość.
 *
 * Scena ma dwa niepowodzenia, bo „rozwija się sam" musi znaczyć „za każdym razem jedno",
 * a nie „po pierwszym błędzie już wszystko".
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { kinds } from './kinds';
import type { Feed } from './model';
import { createFeed } from './model';

/** Dziewięć rodzajów rozwiniętych domyślnie [T2 §7.3 reguła 2] — proza, pytania, błędy, struktura. */
const OPEN = [
  'agent',
  'asked',
  'done',
  'handoff',
  'note',
  'problem',
  'run',
  'step',
  /* 2026-08-19 — tura CZLOWIEKA. Nalezy do prozy i dlatego jest rozwinieta: wiersz, ktory
   * trzeba rozwinac, zeby przeczytac wlasne zdanie, jest zwinieta wlasna wypowiedzia.
   * Powod, dla ktorego ten rodzaj w ogole powstal, stoi przy `Line::Told` po stronie Rusta. */
  'told',
];

/** Sześć zwiniętych — mechanika. */
const SHUT = ['edit', 'memory', 'ran', 'read', 'search', 'stepState', 'thinking'];

/** Czterdzieści linii wyjścia, każda rozpoznawalna po numerze. */
const OUTPUT = Array.from({ length: 40 }, (_, i) => 'output line ' + String(i + 1));

const AGENT = 'Forge';

/**
 * Scena kryterium: dwa niepowodzenia, a między nimi i wokół nich zwykła mechanika.
 *
 * Funkcja, nie stała — świeże obiekty na każdy przypadek. Współdzielone linie znaczyłyby, że
 * implementacja mutująca wejście przecieka z jednego przypadku do drugiego, a wtedy czerwień
 * pokazuje się w innym miejscu niż przyczyna.
 *
 * Odstępy są trzysekundowe, żeby okno sklejania nie miało tu nic do roboty: to jest scena
 * o zwijaniu, a sklejanie ma własne kryterium.
 */
function scene(): readonly FeedLine[] {
  return [
    line.note(1, 0, AGENT, 'Starting on the header row.'),
    line.ran(2, 3_000, AGENT, 'Ran tests', true, ['40 rows, no problems']),
    line.edit(3, 6_000, AGENT, 'src/parser.rs', 12, 4),
    line.ran(4, 9_000, AGENT, 'Ran the build', false, OUTPUT),
    line.read(5, 12_000, AGENT, 'src/main.rs'),
    line.edit(6, 15_000, AGENT, 'src/quote.rs', 3, 1),
    line.ran(7, 18_000, AGENT, 'Ran tests', false, OUTPUT),
  ];
}

function openIds(feed: Feed): Set<number> {
  return new Set(feed.view.history.filter((row) => row.expanded).map((row) => row.id));
}

/** Co się zmieniło między dwoma zbiorami — w obie strony. */
function changed(before: Set<number>, after: Set<number>): number[] {
  return [...new Set([...before, ...after])].filter((id) => before.has(id) !== after.has(id));
}

function rowFor(feed: Feed, id: number) {
  const row = feed.view.history.find((candidate) => candidate.ids.includes(id));
  if (row === undefined) throw new Error('no history row carries line ' + String(id));
  return row;
}

describe('collapsed by default; a failure opens itself and nothing else', () => {
  it('opens exactly nine kinds by default and shuts exactly seven', () => {
    const registry = kinds();
    const open = Object.entries(registry)
      .filter(([, entry]) => entry.expanded)
      .map(([kind]) => kind)
      .sort();
    const shut = Object.entries(registry)
      .filter(([, entry]) => !entry.expanded)
      .map(([kind]) => kind)
      .sort();

    expect(
      open,
      'prose, questions, failures and structure are visible; mechanics are not [T2 §7.3 rule 2]. ' +
        'The two lists are written out rather than counted, because nine of the wrong ones is ' +
        'still nine. Which kinds open is a design decision [T2 §7.3 rule 2], not something the ' +
        'wire can be asked about — unlike the SET of kinds, which kinds.test.ts reads from the mirror.',
    ).toEqual(OPEN);
    expect(shut, 'and the other seven stay shut until somebody asks').toEqual(SHUT);
  });

  it('opens the failed line and leaves its neighbours alone', () => {
    const feed = createFeed(sealedScroller());
    const SCENE = scene();
    feed.appendLines(SCENE.slice(0, 3));
    const before = openIds(feed);

    feed.appendLines(SCENE.slice(3, 4));
    const after = openIds(feed);

    expect(
      changed(before, after),
      'a failure opens ITSELF [T2 §7.3 rule 3]. One id in, none out. Opening the whole stream ' +
        'after the first failure is panic mode, and panic mode is the wall of text this rule ' +
        'exists to prevent',
    ).toEqual([rowFor(feed, 4).id]);
    expect(
      rowFor(feed, 2).expanded,
      'the neighbouring run that worked stays shut — it has nothing anybody needs to read',
    ).toBe(false);
    expect(rowFor(feed, 3).expanded, 'and so does the edit next to it').toBe(false);
  });

  it('shows the LAST twenty lines of output, not the first', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines(scene().slice(0, 4));
    const failed = rowFor(feed, 4);

    expect(failed.output.length, 'the last twenty lines of output [T2 §7.3 rule 3]').toBe(20);
    expect(
      failed.output[0],
      'the FIRST line shown is number 21 of 40. slice(0, 20) instead of slice(-20) shows the ' +
        'start of the log — the half that never carries the reason — and passes every check ' +
        'that only counts rows',
    ).toBe('output line 21');
    expect(failed.output.at(-1), 'and the last line shown is the last line there was').toBe(
      'output line 40',
    );
  });

  it('opens the second failure too, and still only that one', () => {
    const feed = createFeed(sealedScroller());
    const SCENE = scene();
    feed.appendLines(SCENE.slice(0, 6));
    const before = openIds(feed);

    feed.appendLines(SCENE.slice(6, 7));
    const after = openIds(feed);

    expect(
      changed(before, after),
      'two failures, two rows opened, one at a time. A rule that only holds for the first one ' +
        'is a rule that quietly becomes panic mode later in the run',
    ).toEqual([rowFor(feed, 7).id]);
    expect(
      rowFor(feed, 5).expanded || rowFor(feed, 6).expanded,
      'the read and the edit that sit between the two failures are still shut',
    ).toBe(false);
  });
});
