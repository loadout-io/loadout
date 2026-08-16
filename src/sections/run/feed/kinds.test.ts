/* Kryterium 2: czternaście rodzajów linii i ani jednego więcej.
 *
 * `expect(Object.keys(kinds()).length).toBe(14)` jest słabe dwa razy. Raz: przechodzi dla
 * czternastu ZŁYCH nazw. Dwa: przechodzi też wtedy, gdy obok rejestru stoi gałąź `default`,
 * która renderuje to, czego nie rozpoznała — a wtedy pierwszy nowy typ zdarzenia od vendora
 * wyświetla surowy enum z drutu na ekranie (niezmiennik 14). Dlatego zbiór kluczy porównuje
 * się z literałem, a nie liczy, i dlatego druga połowa tego pliku podaje modelowi dwa rodzaje
 * spoza zbioru i przechodzi po WSZYSTKICH polach tekstowych wyniku.
 *
 * Zbiór jest wypisany tutaj ręcznie i to jest celowe. Pętla po `kinds()` pytałaby rejestr
 * o zdanie na własny temat i przeszłaby dla rejestru pustego.
 */
import { describe, expect, it } from 'vitest';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { kinds } from './kinds';
import { createFeed } from './model';

/** Czternaście rodzajów [T2 §7.2], posortowane. */
const EXPECTED = [
  'agent',
  'asked',
  'done',
  'edit',
  'handoff',
  'memory',
  'note',
  'problem',
  'ran',
  'read',
  'run',
  'search',
  'step',
  'thinking',
];

/** Jedyny rodzaj, który idzie do strefy TERAZ zamiast do historii. */
const LIVE = 'thinking';

/** Enumy prosto z drutu — dokładnie to, co przyjdzie, gdy vendor doda typ zdarzenia. */
const FOREIGN = ['tool_use', 'stream_event'];

/** Każdy string, jaki da się wyłuskać z wartości, jak głęboko by nie siedział. */
function textIn(value: unknown, found: string[] = []): string[] {
  if (typeof value === 'string') {
    found.push(value);
  } else if (Array.isArray(value)) {
    for (const item of value) textIn(item, found);
  } else if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value as Record<string, unknown>)) textIn(item, found);
  }
  return found;
}

describe('fourteen kinds of line and not one more', () => {
  it('carries exactly the fourteen keys, compared as a sorted list', () => {
    expect(
      Object.keys(kinds()).sort(),
      'counting to fourteen passes for fourteen wrong names. The set itself is the check, and ' +
        'it is closed [T2 §7.2]: a fifteenth key means somebody taught the view a word the ' +
        'vocabulary table never agreed to',
    ).toEqual(EXPECTED);
  });

  it('sends one kind to the NOW zone and the other thirteen to the history', () => {
    const registry = kinds();
    const live = Object.entries(registry)
      .filter(([, entry]) => entry.route === 'now')
      .map(([kind]) => kind);

    expect(
      live,
      'Thinking… is a status, not a line [T2 §7.3 rule 5], and it is the ONLY one. A second ' +
        'kind in the live slot means two facts fighting over one region (invariant 13)',
    ).toEqual([LIVE]);

    for (const kind of EXPECTED) {
      if (kind === LIVE) continue;
      expect(
        registry[kind as keyof typeof registry]?.route,
        kind + ' belongs in the history, where lines are appended and stay',
      ).toBe('history');
    }
  });

  for (const kind of FOREIGN) {
    it('drops a line whose kind is ' + kind + ', and says nothing about it', () => {
      const feed = createFeed(sealedScroller());
      feed.appendLines([line.note(1, 0, 'Forge', 'Starting on the header row.')]);
      const before = feed.view.history.length;

      const dropped = feed.appendLines([line.foreign(2, 100, 'Forge', kind)]);

      expect(dropped, 'nothing entered the history, so nothing comes back').toEqual([]);
      expect(
        feed.view.history.length,
        'a kind outside the registry is dropped. Never thrown: a vendor adds event types every ' +
          'week and without warning, and an exception here takes the whole view rather than ' +
          'one line',
      ).toBe(before);
      expect(
        textIn(feed.view).filter((text) => text.includes(kind)),
        'and the word itself reaches no text the view carries — not a label, not a row, not ' +
          'the live slot. A fallback renderer that prints what it did not recognise is how a ' +
          'wire word lands on a screen a human reads (invariant 14)',
      ).toEqual([]);
    });
  }

  it('survives an unknown kind without throwing at all', () => {
    const feed = createFeed(sealedScroller());
    expect(
      () => feed.appendLines(FOREIGN.map((kind, i) => line.foreign(i + 1, i * 100, 'Forge', kind))),
      'one unreadable row may cost that row and nothing else',
    ).not.toThrow();
  });
});
