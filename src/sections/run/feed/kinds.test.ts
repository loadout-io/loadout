/* Kryterium 2: rejestr widoku zna DOKŁADNIE te rodzaje, które umie wyprodukować drut.
 *
 * `expect(Object.keys(kinds()).length).toBe(14)` jest słabe dwa razy. Raz: przechodzi dla
 * czternastu ZŁYCH nazw. Dwa: przechodzi też wtedy, gdy obok rejestru stoi gałąź `default`,
 * która renderuje to, czego nie rozpoznała — a wtedy pierwszy nowy typ zdarzenia od vendora
 * wyświetla surowy enum z drutu na ekranie (niezmiennik 14). Dlatego zbiór kluczy porównuje
 * się ze ZBIOREM, a nie liczy, i dlatego druga połowa tego pliku podaje modelowi dwa rodzaje
 * spoza zbioru i przechodzi po WSZYSTKICH polach tekstowych wyniku.
 *
 * 2026-08-18 — ZBIÓR BYŁ TU WPISANY Z PALCA I TO BYŁA WADA, nie decyzja. Uzasadnienie brzmiało
 * „pętla po `kinds()` pytałaby rejestr o zdanie na własny temat i przeszłaby dla rejestru
 * pustego" — i jest poprawne, tylko wyciąga zły wniosek. Rejestr ma DRUGIE, niezależne źródło:
 * lustro drutu w `src/ipc/types.ts`. Porównanie z nim nie jest samozwrotne (rejestr pusty
 * przestaje się zgadzać, lista pusta też) i zapala się SAMO, kiedy rodzaj dochodzi po stronie
 * Rusta. Wersja z literałem przechodziła dopóty, dopóki ktoś nie zapomniał go dopisać — a wtedy
 * wiersz z drutu wypadał z widoku po cichu, czyli dokładnie ta awaria, przed którą to stoi.
 * Punkt (a) niżej dowodzi najpierw, że lustro W OGÓLE coś oddało: dwa puste zbiory są równe.
 */
import { describe, expect, it } from 'vitest';
import { WIRE_KINDS } from '../../../ipc/types';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { kinds } from './kinds';
import { createFeed } from './model';

/** Rodzaje, jakie umie wyprodukować drut — CZYTANE z lustra, nie przepisane [T2 §7.2]. */
const EXPECTED = [...WIRE_KINDS].sort();

/**
 * Rodzaje, które nie wchodzą do historii, a zajmują stały slot [T2 §7.3 reguła 5].
 *
 * Trzy i każdy odpowiada na inne pytanie: `thinking` mówi, co agent robi, `stepState` — na
 * czym stoi krok, a `stepCarriedOn` — czy scheduler wykonał dla jego porażki „jedź dalej”.
 */
const LIVE = ['stepCarriedOn', 'stepState', 'thinking'];

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

describe('the view knows exactly the kinds the wire can send', () => {
  it('reads a real set of kinds out of the wire mirror first', () => {
    expect(
      EXPECTED.length,
      'the wire mirror in src/ipc/types.ts gave back no kinds at all. Everything below hangs ' +
        'off that list, and an empty one turns the comparison into `[] equals []` — which is ' +
        'exactly the shape of green this criterion exists to end.',
    ).toBeGreaterThanOrEqual(14);
  });

  it('carries exactly the keys the wire declares, compared as a sorted list', () => {
    expect(
      Object.keys(kinds()).sort(),
      'counting to a number passes for that many wrong names. The set itself is the check, and ' +
        'it is closed [T2 §7.2]: a key the wire never declares means somebody taught the view ' +
        'a word the vocabulary table never agreed to, and a kind the wire DOES declare and the ' +
        'registry does not means that line falls out of the view in silence.',
    ).toEqual(EXPECTED);
  });

  it('keeps the standing slot to the two facts that belong there, and history for the rest', () => {
    const registry = kinds();
    const live = Object.entries(registry)
      .filter(([, entry]) => entry.route === 'now')
      .map(([kind]) => kind)
      .sort();

    expect(
      live,
      'Thinking… is a status, not a line [T2 §7.3 rule 5], and so is which step the run stands ' +
        'on, and whether its failure explicitly carried on. These are state facts, never rows ' +
        'of history (invariant 13).',
    ).toEqual(LIVE);

    for (const kind of EXPECTED) {
      if (LIVE.includes(kind)) continue;
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
