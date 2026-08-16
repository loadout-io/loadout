/* Kryterium 1: widok nigdy nie przewija się sam.
 *
 * To jest ten moment, w którym teza produktu („widok nie przyrasta, aktualizuje się w miejscu")
 * albo jest prawdą, albo jest zdaniem w DESIGN.md. Sprawdzian z DESIGN §1 brzmi wprost: jeśli
 * podczas biegu czterech agentów widok przewinął się choć raz sam z siebie — projekt jest złamany.
 *
 * `expect(scroller.calls.length).toBe(0)` SAMO w sobie jest słabe i trzeba to powiedzieć głośno:
 * przechodzi dla implementacji, która nie przewija, bo nie ma czego przypiąć — a wtedy nowe linie
 * chowają się pod krawędzią i użytkownik traci je tak samo, tylko ciszej. Dlatego czerwień tego
 * pliku pilnują trzy rzeczy naraz:
 *   - zero wywołań portu przy DWÓCH różnych pozycjach atrapy (na dole i 300 px wyżej): kto pyta
 *     „czy jestem na dole", żeby zdecydować, ten już zapisał odczyt w `calls`,
 *   - cztery wiersze w strefie TERAZ po KAŻDEJ paczce: `lines.slice(-4)` daje ten sam zrzut
 *     ekranu i pełznie o wiersz na każde zdarzenie,
 *   - tożsamość referencyjna `view.history`: paczka samych `thinking` nie ma prawa jej ruszyć,
 *     a paczka z prawdziwą linią ma oddać dokładnie ten obiekt, który wrócił z `appendLines`.
 */
import { describe, expect, it } from 'vitest';
import { line } from './fixtures/lines';
import { AGENTS, ASKED_AT, agentAt, run200 } from './fixtures/run-200';
import type { Scroller } from './model';
import { createFeed } from './model';

/** Jedno dotknięcie portu przewijania — także ODCZYT pozycji. */
interface Call {
  readonly method: string;
  readonly arg: number | null;
}

interface Recorder extends Scroller {
  readonly calls: readonly Call[];
}

/**
 * Atrapa portu, która zapisuje każde wywołanie i raportuje zadaną pozycję.
 *
 * Odczyt `scrollTop` też jest wywołaniem i to jest sedno: implementacja „przewijaj tylko, gdy
 * użytkownik jest na dole" musi najpierw zapytać, gdzie on jest. Atrapa, która milczy o odczycie,
 * przepuściłaby ją przy pozycji „300 px w górę" i złapała dopiero przy „na dole" — czyli w tym
 * jednym przebiegu, w którym akurat jest zielono.
 */
function recorder(top: number): Recorder {
  const calls: Call[] = [];
  return {
    calls,
    scrollTop(): number {
      calls.push({ method: 'scrollTop', arg: null });
      return top;
    },
    scrollTo(to: number): void {
      calls.push({ method: 'scrollTo', arg: to });
    },
    scrollIntoView(id: number): void {
      calls.push({ method: 'scrollIntoView', arg: id });
    },
  };
}

/** Liczności rodzajów w skrypcie, wypisane z treści kryterium — nie czytane ze skryptu. */
const SHAPE: ReadonlyArray<readonly [string, number]> = [
  ['thinking', 63],
  ['read', 60],
  ['search', 20],
  ['edit', 18],
  ['ran', 12],
  ['note', 10],
  ['handoff', 4],
  ['agent', 4],
  ['step', 3],
  ['memory', 3],
  ['run', 1],
  ['asked', 1],
  ['problem', 1],
];

const BATCH = 10;

function counted(kinds: readonly string[]): Map<string, number> {
  const tally = new Map<string, number>();
  for (const kind of kinds) tally.set(kind, (tally.get(kind) ?? 0) + 1);
  return tally;
}

describe('the run view never scrolls itself', () => {
  it('runs on the script the rest of this file leans on', () => {
    const script = run200();
    const tally = counted(script.map((row) => row.kind));

    expect(script.length, 'the script is 200 events long').toBe(200);
    for (const [kind, howMany] of SHAPE) {
      expect(tally.get(kind), 'the script carries ' + String(howMany) + ' of ' + kind).toBe(
        howMany,
      );
    }
    expect(
      new Set(script.map((row) => row.agent)).size,
      'four agents work in this run and no more: a fifth name would put a fifth row in the ' +
        'NOW zone and the row count below would stop meaning what it says',
    ).toBe(AGENTS.length);
    expect(
      new Set(script.slice(0, BATCH).map((row) => row.agent)).size,
      'all four agents show up inside the first batch. Otherwise "four rows after every batch" ' +
        'would be measuring how fast agents arrive rather than the shape of the NOW zone',
    ).toBe(AGENTS.length);
    expect(
      script[ASKED_AT - 1]?.kind,
      'the one question to a human sits at a fixed place in the script',
    ).toBe('asked');
  });

  for (const [where, top] of [
    ['parked at the bottom', 0],
    ['scrolled 300 px up', 300],
  ] as const) {
    it('touches the scroll port zero times with the view ' + where, () => {
      const scroller = recorder(top);
      const feed = createFeed(scroller);
      const script = run200();

      for (let i = 0; i < script.length; i += BATCH) {
        feed.appendLines(script.slice(i, i + BATCH));
        expect(
          feed.view.now.rows.length,
          'one row per agent, four agents, after every single batch. A NOW zone built as ' +
            'lines.slice(-4) is impossible to tell apart in a picture and pushes the history ' +
            'up by a row on every event, so the view creeps without one scroll call',
        ).toBe(AGENTS.length);
      }

      expect(
        [...feed.view.now.rows].map((row) => row.agent).sort(),
        'the four rows are the four agents, keyed by who they belong to — not the four newest ' +
          'lines wearing agent names',
      ).toEqual([...AGENTS].sort());

      expect(
        scroller.calls,
        'two hundred events, twenty batches, and not one call to the scroll port. Sticking to ' +
          'the bottom is the job of the layout (column-reverse), not of a line of script. ' +
          'Reading the position counts too: whoever asks "am I at the bottom" is deciding ' +
          'whether to yank the page, and the answer to that is never',
      ).toEqual([]);
    });
  }

  it('scrolls exactly once, and only when the button asks it to', () => {
    const scroller = recorder(300);
    const feed = createFeed(scroller);
    const script = run200();
    for (let i = 0; i < script.length; i += BATCH) feed.appendLines(script.slice(i, i + BATCH));

    feed.jumpToNewest();

    expect(
      scroller.calls.length,
      'jumpToNewest is the one imperative road to the scroll port and it has its own button, ' +
        'so it costs exactly one call',
    ).toBe(1);
    expect(
      scroller.calls[0]?.method,
      'that one call has to move the view. Reading the position and doing nothing is a button ' +
        'with no handler wearing a handler',
    ).not.toBe('scrollTop');
  });

  it('leaves the history untouched for a batch that is nothing but Thinking…', () => {
    const feed = createFeed(recorder(0));
    feed.appendLines([line.note(1, 0, 'Forge', 'Starting on the header row.')]);

    const before = feed.view.history;
    for (let i = 0; i < 63; i += 1) {
      feed.appendLines([line.thinking(100 + i, 1000 + i * 10, agentAt(i))]);
      expect(
        feed.view.history,
        'Thinking… is a status, not a line [T2 §7.3 rule 5]. Sixty-three of them may not ' +
          'produce a new history array: a fresh array asks React to redraw the whole history ' +
          'for something that never entered it, and at four agents that is the whole cost',
      ).toBe(before);
    }

    expect(
      feed.view.history.some((row) => row.kind === 'thinking'),
      'and none of them may land in the history either — hiding them with display:none keeps ' +
        'the row in the DOM, in memory and in the 10 000-line budget (invariant 15)',
    ).toBe(false);
    expect(
      Array.isArray(feed.view.now.thinking),
      'the NOW zone holds ONE thinking slot, never a list of them (invariant 13: one fact, ' +
        'one place). A list is the same wall of text with a different name',
    ).toBe(false);
    expect(
      typeof feed.view.now.thinking,
      'after sixty-three of them the slot is live and holds the agent it belongs to',
    ).toBe('string');
  });

  it('hands back the very row it appended, and only then a new history', () => {
    const feed = createFeed(recorder(0));
    feed.appendLines([line.note(1, 0, 'Forge', 'Starting on the header row.')]);
    for (let i = 0; i < 63; i += 1) {
      feed.appendLines([line.thinking(100 + i, 1000 + i * 10, agentAt(i))]);
    }

    const before = feed.view.history;
    const appended = feed.appendLines([line.note(2, 2000, 'Needle', 'The header row is fine.')]);

    expect(
      feed.view.history,
      'a real history line does change the array — the zone that grows is allowed to grow',
    ).not.toBe(before);
    expect(appended.length, 'one history line in, one row back').toBe(1);
    expect(
      feed.view.history.at(-1),
      'and the row at the end of the history IS the row that came back: reference identity is ' +
        'what tells "appended one row" apart from "rebuilt the list and it happens to look the ' +
        'same". The second one is correct in value and ruinous for React',
    ).toBe(appended[0]);
    expect(
      feed.view.now.thinking,
      'a real line lands, the Thinking… slot goes quiet [T2 §7.2 row 4]',
    ).toBeNull();
  });
});
