/* Skok bez modyfikatora nie ma prawa odpalić się pod palcem kogoś, kto pisze.
 *
 * TO JEST NAJCZĘSTSZA WADA TEGO WZORCA i dlatego stoi jako pierwsze kryterium tej palety.
 * Litery `g` i `r` w słowie „grand" są dokładnie tymi samymi znakami, co skrót `G R`; jedyne,
 * co je odróżnia, to gdzie stoi ognisko. Reguła zapisana wprost w nasłuchu byłaby regułą,
 * której żadne kryterium nie umie dotknąć — to repo nie ma jsdom.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI. Samo „pisanie nie skacze" przechodzi na kodzie, który nie
 * skacze NIGDY — czyli na palecie, której nie ma. Każdy przypadek niżej ma więc parę: te same
 * litery, to samo uzbrojenie, ognisko poza polem, i wtedy skok MA się wydarzyć.
 */
import { describe, expect, it } from 'vitest';

import { SECTIONS } from '../sections';
import { JUMPS, focusedShape, insideMove, moveFor, stepped, takesTyping } from './keys';
import type { Focused, Move } from './keys';

const PLAIN = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false } as const;

/** Ognisko w zwykłym polu tekstowym — dokładnie to, co ma na ekranie piszący człowiek. */
const IN_A_FIELD: Focused = { tagName: 'INPUT', isContentEditable: false };

/** Ognisko nigdzie: człowiek patrzy na ekran i niczego nie wypełnia. */
const NOWHERE = null;

/** Litery jednego słowa, wpisywane po kolei, z zapadką przenoszoną między naciśnięciami. */
function typedOut(word: string, focused: Focused | null): Move['move'][] {
  let waiting = false;
  return [...word].map((letter) => {
    const next = moveFor({ ...PLAIN, key: letter }, focused, waiting);
    waiting = next.move === 'wait';
    return next.move;
  });
}

describe('the keyboard tells typing apart from a shortcut', () => {
  it('never moves the screen while the word "grand" is being typed into a field', () => {
    expect(
      typedOut('grand', IN_A_FIELD),
      'every letter of a word typed into a field has to mean itself. The moment "g" arms a ' +
        'jump and "r" takes it, half the word is left behind on a screen nobody asked for.',
    ).toEqual(['none', 'none', 'none', 'none', 'none']);

    /* KONTROLA PRZECIW PUSTEJ ASERCJI, w tym samym kryterium. Bez tej linii zdanie wyżej jest
       prawdziwe także o palecie, która nie skacze NIGDY — czyli o kodzie, którego nie ma.
       Te same pięć liter, ta sama zapadka, ognisko poza polem: „g" uzbraja, „r" skacze. */
    expect(typedOut('grand', NOWHERE)).toEqual(['wait', 'jump', 'none', 'none', 'none']);
  });

  it('takes the same two letters as a jump when nothing has focus', () => {
    /* Para do przypadku wyżej. Bez niej całe kryterium przechodzi na palecie, której nie ma. */
    expect(moveFor({ ...PLAIN, key: 'g' }, NOWHERE, false)).toEqual({ move: 'wait' });
    expect(moveFor({ ...PLAIN, key: 'r' }, NOWHERE, true)).toEqual({
      move: 'jump',
      section: 'run',
    });
    expect(typedOut('grand', NOWHERE)[1]).toBe('jump');
  });

  it('sends every promised letter to the section it names', () => {
    const promised = [
      ['r', 'run'],
      ['w', 'workflows'],
      ['a', 'agents'],
      ['k', 'knowledge'],
      ['t', 'triggers'],
      ['s', 'settings'],
    ] as const;
    for (const [letter, section] of promised) {
      expect(moveFor({ ...PLAIN, key: letter }, NOWHERE, true), letter).toEqual({
        move: 'jump',
        section,
      });
    }
  });

  it('reads the letters off the one list of sections instead of keeping a second one', () => {
    /* Niezmiennik 13: liter jest tyle, ile RÓŻNYCH pierwszych liter w rejestrze — a nie tyle,
       ile sekcji. Do 2026-08-31 `skills` i `settings` zaczynały się tą samą literą i `S`
       należało do pierwszej z nich; po scaleniu Skills i Memory w Knowledge sześć sekcji ma
       sześć różnych pierwszych liter, więc kolizji nie ma ani jednej. Ta asercja nie jest przez
       to pusta: gdyby ktoś dopisał sekcję kolidującą, `JUMPS.size` przestałoby się zgadzać
       z liczbą sekcji, a wiersz niżej mówi, KTÓRA z nich bierze wtedy literę. */
    const first = new Set(SECTIONS.map((entry) => entry.id.slice(0, 1)));
    expect(JUMPS.size).toBe(first.size);
    expect(
      JUMPS.size,
      'six sections with six different first letters, so nobody loses a jump',
    ).toBe(SECTIONS.length);
    for (const entry of SECTIONS) {
      const letter = entry.id.slice(0, 1);
      const owner = [...SECTIONS].find((one) => one.id.slice(0, 1) === letter)?.id;
      expect(
        JUMPS.get(letter),
        letter +
          ' belongs to the section standing higher in the ' +
          'registry, and the registry order is part of the contract',
      ).toBe(owner);
    }
  });

  it('opens the list with the one shortcut that carries a modifier, field or no field', () => {
    expect(moveFor({ ...PLAIN, metaKey: true, key: 'k' }, IN_A_FIELD, false)).toEqual({
      move: 'open',
    });
    expect(moveFor({ ...PLAIN, ctrlKey: true, key: 'K' }, NOWHERE, false)).toEqual({
      move: 'open',
    });
  });

  it('shows the shortcuts on "?" only when the question mark is not being typed', () => {
    expect(moveFor({ ...PLAIN, shiftKey: true, key: '?' }, IN_A_FIELD, false)).toEqual({
      move: 'none',
    });
    expect(moveFor({ ...PLAIN, shiftKey: true, key: '?' }, NOWHERE, false)).toEqual({
      move: 'shortcuts',
    });
  });

  it('disarms on a letter that names no section, instead of waiting forever', () => {
    expect(moveFor({ ...PLAIN, key: 'q' }, NOWHERE, true)).toEqual({ move: 'none' });
    /* Para: litera, która sekcję nazywa, ma przy tym samym uzbrojeniu skoczyć. */
    expect(moveFor({ ...PLAIN, key: 'k' }, NOWHERE, true)).toEqual({
      move: 'jump',
      section: 'knowledge',
    });
  });

  it('counts a text area and an editable block as places where somebody is typing', () => {
    expect(takesTyping({ tagName: 'TEXTAREA', isContentEditable: false })).toBe(true);
    expect(takesTyping({ tagName: 'DIV', isContentEditable: true })).toBe(true);
    expect(takesTyping({ tagName: 'BUTTON', isContentEditable: false })).toBe(false);
    expect(takesTyping(null)).toBe(false);
  });

  it('reads those two facts off a real element without naming a browser type', () => {
    const button = { tagName: 'BUTTON' } as unknown as Element;
    expect(focusedShape(button)).toEqual({ tagName: 'BUTTON', isContentEditable: false });
    expect(focusedShape(null)).toBeNull();
  });

  it('walks the list with the arrows, picks with Enter and leaves with Escape', () => {
    expect(insideMove({ ...PLAIN, key: 'ArrowDown' })).toEqual({ move: 'step', by: 1 });
    expect(insideMove({ ...PLAIN, key: 'ArrowUp' })).toEqual({ move: 'step', by: -1 });
    expect(insideMove({ ...PLAIN, key: 'Enter' })).toEqual({ move: 'choose' });
    expect(insideMove({ ...PLAIN, key: 'Escape' })).toEqual({ move: 'close' });
    expect(insideMove({ ...PLAIN, key: 'x' })).toEqual({ move: 'none' });
  });

  it('wraps the highlight instead of parking it outside the list', () => {
    expect(stepped(2, 1, 3)).toBe(0);
    expect(stepped(0, -1, 3)).toBe(2);
    expect(stepped(0, 1, 0)).toBe(0);
  });
});
