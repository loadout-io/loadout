/* Kryterium 4: widok jednego agenta to TEN SAM strumień z filtrem, a nie druga jego derywacja.
 *
 * `expect(rows.every((r) => r.agent === 'Forge')).toBe(true)` przechodzi dla implementacji,
 * która przelicza strumień od nowa dla jednego agenta. Każdy wiersz naprawdę należy do
 * Forge'a, asercja jest zielona — a widok agenta i strumień główny dzielą linie na grupy
 * inaczej i człowiek dostaje dwie różne odpowiedzi na jedno pytanie, bez żadnej wskazówki,
 * która z nich jest prawdziwa.
 *
 * Rozróżniają to trzy porównania między obiema derywacjami, ostatnie najostrzej:
 *   1. identyfikatory jako PODCIĄG — ta sama kolejność, te same granice,
 *   2. pary (identyfikator grupy, licznik) — identyczne, więc żaden wiersz nie skleił się
 *      inaczej niż w strumieniu głównym,
 *   3. flagi rozwinięcia PO ręcznym przestawieniu jednej z nich. Tego przeliczenie z linii
 *      nie ma jak odtworzyć: rozwinięcie jest stanem wiersza, który postawił człowiek,
 *      i nie ma go w niczym, z czego dałoby się wiersz policzyć jeszcze raz.
 *
 * Jedna rzecz sprawdzona i odnotowana, bo kryterium każe ją zgłosić, gdyby wypadła inaczej:
 * model T-08 skleja po PARZE (agent, rodzaj), nie ponad agentami [feed/model.ts, `groups`],
 * więc obie derywacje da się uzgodnić i nie ma tu defektu T-08 do zgłoszenia. Scena poniżej
 * jest dokładnie tą, która by go pokazała — sześć odczytów dwóch agentów na przemian
 * w jednym oknie dwóch sekund.
 */
import { describe, expect, it } from 'vitest';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import type { Feed } from '../feed/model';
import { createFeed } from '../feed/model';
import { sessionFeed } from './filter';

const FORGE = 'Forge';
const NEEDLE = 'Needle';
/** Pod-agent: Forge rozpuścił go w trakcie biegu i nie ma go w żadnym workflow. */
const SCOUT = 'Scout';

/**
 * Forge i Needle na przemian po trzy odczyty w oknie 2 s, notatka Forge'a pomiędzy,
 * potem druga grupa Forge'a już za oknem i jedna linia pod-agenta w środku niej.
 */
function alternating(): Feed {
  const feed = createFeed(sealedScroller());
  feed.appendLines([
    line.read(1, 0, FORGE, 'src/parser.rs'),
    line.read(2, 200, NEEDLE, 'tests/parser.rs'),
    line.read(3, 400, FORGE, 'src/main.rs'),
    line.read(4, 600, NEEDLE, 'tests/main.rs'),
    line.read(5, 800, FORGE, 'src/quote.rs'),
    line.read(6, 1_000, NEEDLE, 'tests/quote.rs'),
    line.note(7, 1_200, FORGE, 'The header row carries a stray quote.'),
    line.read(8, 3_500, FORGE, 'src/csv.rs'),
    line.read(9, 3_600, SCOUT, 'docs/csv-edge-cases.md'),
    line.read(10, 3_800, FORGE, 'src/lex.rs'),
  ]);
  return feed;
}

function isSubsequenceOf(small: readonly number[], big: readonly number[]): boolean {
  let at = 0;
  for (const id of big) {
    if (small[at] === id) at += 1;
  }
  return at === small.length;
}

describe('one agent gets the same rows the whole run got, only fewer', () => {
  it('reads as a subsequence of the run, in the same order', () => {
    const feed = alternating();
    const mine = sessionFeed(feed.view, FORGE).map((row) => row.id);
    const all = feed.view.history.map((row) => row.id);

    expect(mine.length, 'and it is not empty, which would satisfy any subsequence').toBe(3);
    expect(
      isSubsequenceOf(mine, all),
      'same identifiers, same order. A second derivation can be right about every row and ' +
        'still put the boundaries somewhere else',
    ).toBe(true);
  });

  it('folds the rows exactly the way the whole run folded them', () => {
    const feed = alternating();
    const mine = sessionFeed(feed.view, FORGE).map((row) => [row.id, row.count]);
    const theirs = feed.view.history
      .filter((row) => row.agent === FORGE)
      .map((row) => [row.id, row.count]);

    expect(
      mine,
      'pairs of row identifier and count, from both derivations. Six reads by two agents ' +
        'inside one window are the case where a fresh count and the real one come apart, ' +
        'and the screen then says "Read 3 files" beside "Read 6 files" with nothing to ' +
        'tell a person which is true',
    ).toEqual(theirs);
  });

  it('carries an opening a person made by hand, which no recount could know about', () => {
    const feed = alternating();
    const folded = feed.view.history.find((row) => row.id === 1);
    expect(folded?.expanded, 'reads start closed [T2 §7.3 rule 2]').toBe(false);

    feed.toggle(1);

    const mine = sessionFeed(feed.view, FORGE);
    const theirs = feed.view.history.filter((row) => row.agent === FORGE);

    expect(
      mine.map((row) => row.expanded),
      'the opened row is opened here too. This flag is a state a person put there; nothing ' +
        'in the lines it was built from remembers it, so a derivation that starts over ' +
        'quietly closes the row the person just opened',
    ).toEqual(theirs.map((row) => row.expanded));
    expect(mine.find((row) => row.id === 1)?.expanded).toBe(true);
  });

  it('never lets a line from another agent through', () => {
    const feed = alternating();
    const mine = sessionFeed(feed.view, FORGE);

    expect(
      mine.map((row) => row.agent),
      'every row belongs to the agent that was asked about — necessary, and on its own not ' +
        'nearly enough',
    ).toEqual([FORGE, FORGE, FORGE]);
    expect(mine.map((row) => row.id)).not.toContain(9);
  });

  it('gives a sub-agent line to the child once and echoes it into the run once', () => {
    const feed = alternating();
    const child = sessionFeed(feed.view, SCOUT).map((row) => row.id);
    const parent = sessionFeed(feed.view, FORGE).map((row) => row.id);
    const all = feed.view.history.map((row) => row.id);

    expect(child.filter((id) => id === 9).length, 'once in the child, never twice').toBe(1);
    expect(
      all.filter((id) => id === 9).length,
      'and one echo line in the main run [T2 §9.3] — the run should show that something ' +
        'happened, without replaying the whole of it a second time',
    ).toBe(1);
    expect(
      parent,
      'the parent did not read that file; a sub-agent did. Folding the child into the ' +
        'parent hands one agent work another one did',
    ).not.toContain(9);
  });
});
