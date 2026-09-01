/* Podświetlenie mówi prawdę o tym, czy Enter przyjmie tę nazwę.
 *
 * # Zamówienie
 *
 * Właściciel, 2026-08-30: „jak to wpisuje w terminal to ma sie podswietlac jakos zajebiscie".
 *
 * # Czego to kryterium pilnuje naprawdę
 *
 * Nie tego, ŻE coś się podświetla — tego, że podświetla się dokładnie to, co Enter przyjmie.
 * Kolor obiecujący nazwę, której polityka startu nie zna, jest gorszy niż brak koloru: człowiek
 * uczy się mu ufać w pierwszej minucie i przestaje czytać odmowy. Dlatego rozpoznanie liczy
 * `typable` — tę samą funkcję, którą liczy `readRunLine`.
 *
 * # Słaba wersja, której tu nie ma
 *
 * „Zwraca jakiś kawałek z `known: true`". Przechodzi dla implementacji, która podświetla KAŻDE
 * drugie słowo, także w zdaniu do lidera — a zdanie do lidera jest tym, co człowiek pisze
 * najczęściej. Rozstrzygają przypadki z linią bez komendy i z nazwą, której nie ma.
 */
import { describe, expect, it } from 'vitest';

import { segments } from './highlight';

/** Nazwy, jakie oddaje katalog — w postaci, w jakiej trzyma je okno. */
const KNOWN = ['Ship a feature', 'review-and-fix'];

/** Skleja kawałki z powrotem: warstwa rysująca robi dokładnie to. */
function whole(line: string): string {
  return segments(line, KNOWN)
    .map((piece) => piece.text)
    .join('');
}

/** Fragmenty uznane za znane. */
function lit(line: string): readonly string[] {
  return segments(line, KNOWN)
    .filter((piece) => piece.known)
    .map((piece) => piece.text);
}

describe('what lights up while a person types', () => {
  it('lights the name Loadout has, in the form they typed it', () => {
    expect(lit('/run ship-a-feature')).toEqual(['ship-a-feature']);
  });

  it('lights it the same when the title was typed instead of the key', () => {
    expect(
      lit('/run Ship a feature'),
      'only the first word is the name — the rest is the task. `Ship` alone is not a name Enter ' +
        'would accept, so it must not light up',
    ).toEqual([]);
  });

  it('leaves a name nobody has dark, so the typo is visible before Enter', () => {
    expect(
      lit('/run shipp-a-feature'),
      'this is the whole point: the person sees the typo while typing, instead of paying a ' +
        'refusal for it',
    ).toEqual([]);
  });

  it('lights the name and nothing of the task behind it', () => {
    expect(lit('/run review-and-fix make the parser stop crashing')).toEqual(['review-and-fix']);
  });

  it('lights nothing in a sentence to the lead', () => {
    expect(
      lit('can you review and fix the parser'),
      'prose is what a person writes most of the time. A rule that looks for names anywhere in ' +
        'the line paints their sentences, and the colour then means nothing',
    ).toEqual([]);
  });

  it('lights nothing for a command that only starts like /run', () => {
    expect(
      lit('/runner ship-a-feature'),
      'the same boundary the start policy uses: without the space, `/runner` is a different ' +
        'word, not /run with a name',
    ).toEqual([]);
  });

  it('never changes a single character of what the person typed', () => {
    for (const line of [
      '',
      '/run',
      '/run ',
      '/run ship-a-feature',
      '/run review-and-fix  build   it ',
      'just talking',
      '/runner x',
    ]) {
      expect(
        whole(line),
        'the layer draws these pieces back to back under the real field. One character lost or ' +
          'added here shifts the wash off the word by exactly that much',
      ).toBe(line);
    }
  });

  it('emits nothing at all for an empty line', () => {
    expect(
      segments('', KNOWN),
      'the density ratchet on visible text can only go down (ARCHITECTURE §7), so the default ' +
        'empty view must not gain one',
    ).toEqual([]);
  });
});
