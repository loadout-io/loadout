/* Kryterium 4: sklejanie sąsiednich linii tego samego rodzaju w oknie 2 s [T2 §7.3 reguła 4].
 *
 * `expect(label).toBe('Read 3 files')` przechodzi dla implementacji BEZ ograniczenia czasowego
 * i BEZ klucza agenta, bo w tym jednym przypadku dane są wygodne. Rozróżniają to trzy dalsze
 * przypadki, każdy nazywa inny sposób, w jaki sklejanie kłamie:
 *
 *   2100 ms       okno liczone od OSTATNIEJ linii grupy nigdy się nie zamyka: przy stałym
 *                 strumieniu odczytów cały bieg schodzi do jednego wiersza „Read 400 files",
 *   inny rodzaj   grupa, której nie zamyka linia innego rodzaju, przeskakuje ponad prozą
 *                 i wiąże ze sobą rzeczy, które w biegu nic ze sobą nie miały,
 *   inny agent    `Read 6 files` przypisane Forge'owi, gdy trzy z tych odczytów zrobił Needle,
 *                 to fałszywa atrybucja pracy — najgorszy możliwy wynik sklejania, bo wygląda
 *                 dokładnie jak wynik poprawny.
 *
 * I jedno na koniec: rozwinięcie musi oddać IDENTYFIKATORY, nie samą liczbę. Sklejanie, które
 * nie umie pokazać, co skleiło, jest po prostu gubieniem.
 */
import { describe, expect, it } from 'vitest';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

const A = 'Forge';
const B = 'Needle';

describe('adjacent lines of one kind inside a two-second window become one', () => {
  it('folds three reads at 0, 500 and 1200 ms into one line that counts them', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([
      line.read(1, 0, A, 'src/parser.rs'),
      line.read(2, 500, A, 'src/main.rs'),
      line.read(3, 1_200, A, 'src/quote.rs'),
    ]);

    expect(feed.view.history.length, 'three reads, one row').toBe(1);
    const row = feed.view.history[0];
    expect(row?.count, 'and the row knows there were three of them').toBe(3);
    expect(
      row?.label,
      'the count is always in the label [T2 risk 3]. "Read 47 files" and "Read 3 files" look ' +
        'the same at a glance only when the number is missing',
    ).toBe('Read 3 files');
    expect(
      row?.ids,
      'opening the row gives back the three original lines, in the order they arrived. A row ' +
        'that can only say "three" has thrown the other two away and called it folding',
    ).toEqual([1, 2, 3]);
  });

  it('closes the window two seconds after the FIRST line of the group', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([
      line.read(1, 0, A, 'src/parser.rs'),
      line.read(2, 1_900, A, 'src/main.rs'),
      line.read(3, 2_100, A, 'src/quote.rs'),
    ]);

    expect(
      feed.view.history.map((row) => row.count),
      'the window runs from the first line of the group, not from the last one. Measured from ' +
        'the last, a steady stream of reads never closes a group and the whole run collapses ' +
        'into one row',
    ).toEqual([2, 1]);
    expect(feed.view.history.map((row) => row.label)).toEqual(['Read 2 files', 'Read 1 file']);
  });

  it('lets a line of another kind close the group', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([
      line.read(1, 0, A, 'src/parser.rs'),
      line.note(2, 200, A, 'The header row carries a stray quote.'),
      line.read(3, 400, A, 'src/main.rs'),
    ]);

    expect(
      feed.view.history.map((row) => row.kind),
      'prose between two reads breaks the group. A group that jumps over it binds together ' +
        'work that had nothing to do with each other in the run',
    ).toEqual(['read', 'note', 'read']);
    expect(feed.view.history.map((row) => row.count)).toEqual([1, 1, 1]);
  });

  it('keys the group by agent as well as by kind', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([
      line.read(1, 0, A, 'src/parser.rs'),
      line.read(2, 500, B, 'src/main.rs'),
      line.read(3, 1_000, A, 'src/quote.rs'),
    ]);

    const mine = feed.view.history.filter((row) => row.agent === A);
    const theirs = feed.view.history.filter((row) => row.agent === B);

    expect(
      mine.map((row) => row.count),
      'the group is keyed by the pair (agent, kind). Reads by two agents inside one window are ' +
        'two rows — putting them in one hands one agent the work another one did, and that ' +
        'reads as true because nothing on the screen contradicts it',
    ).toEqual([2]);
    expect(mine[0]?.ids, 'and the folded row carries its own two lines').toEqual([1, 3]);
    expect(theirs.map((row) => row.count)).toEqual([1]);
    expect(
      feed.view.history.some((row) => row.count === 3),
      'never one row of three: that is the false attribution this case exists to catch',
    ).toBe(false);
  });
});
