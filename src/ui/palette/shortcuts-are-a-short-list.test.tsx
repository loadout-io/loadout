/* Lista skrótów: krótka, przeszukiwalna i wyprowadzona z tego samego miejsca, co klawiatura.
 *
 * DWIE RZECZY, KTÓRYCH TA LISTA NIE MA PRAWA ZROBIĆ. Obiecać skrótu, którego klawiatura nie zna
 * — wtedy człowiek naciska i nic się nie dzieje, a jedyne, co mu zostaje, to wniosek, że nie umie
 * obsługiwać aplikacji. I przemilczeć skrót, który działa — wtedy funkcja istnieje i nikt jej
 * nigdy nie znajdzie. Oba przypadki mają tu asercję, bo oba wynikają z tej samej wady:
 * z drugiej listy skrótów, spisanej ręką.
 *
 * „NIE ŚCIANA" JEST LICZBĄ. Sufit trzynastu wierszy nie jest gustem: lista skrótów dłuższa niż
 * ekran przestaje być odpowiedzią na `?` i staje się dokumentacją, którą się zamyka.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { sectionEntry } from '../sections';
import { JUMPS } from './keys';
import { Palette } from './palette';
import { matchingShortcuts, shortcuts } from './shortcuts';

const NOTHING = (): void => undefined;

/** Ile wierszy wolno mieć tej liście, zanim przestanie być odpowiedzią i stanie się ścianą. */
const NOT_A_WALL = 13;

function drawn(typed: string): string {
  return renderToStaticMarkup(
    <Palette
      showing="shortcuts"
      typed={typed}
      items={[]}
      rows={matchingShortcuts(shortcuts(), typed)}
      at={0}
      unread={false}
      onType={NOTHING}
      onStep={NOTHING}
      onChoose={NOTHING}
      onShow={NOTHING}
      onClose={NOTHING}
    />,
  );
}

function times(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('"?" answers with a list somebody can read in one go', () => {
  it('stays short enough to read without scrolling', () => {
    expect(shortcuts().length).toBeGreaterThan(0);
    expect(shortcuts().length).toBeLessThanOrEqual(NOT_A_WALL);
  });

  it('names every letter the keyboard really takes, and names it by its section label', () => {
    const rows = shortcuts();
    /* Pusta mapa przechodzi każdą pętlę „dla każdego skoku…" i każde porównanie zbiorów. */
    expect(JUMPS.size).toBeGreaterThan(0);
    for (const [letter, section] of JUMPS) {
      const row = rows.find((one) => one.press === 'G ' + letter.toUpperCase());
      expect(row, letter).toBeDefined();
      expect(row?.does).toContain(sectionEntry(section).label);
    }
    /* Druga strona tej samej reguły: ani jednego wiersza o skoku, którego mapa nie zna. */
    const promised = rows
      .filter((one) => one.press.startsWith('G '))
      .map((one) => one.press.slice(2).toLowerCase());
    expect([...promised].sort()).toEqual([...JUMPS.keys()].sort());
  });

  it('tells a person how to open it and how to leave it', () => {
    const press = shortcuts().map((one) => one.press);
    expect(press).toContain('⌘K');
    expect(press).toContain('?');
    expect(press).toContain('Esc');
  });

  it('narrows to what was typed, so a longer list would still be findable', () => {
    const narrowed = matchingShortcuts(shortcuts(), 'knowledge');
    expect(narrowed.length).toBeGreaterThan(0);
    expect(narrowed.length).toBeLessThan(shortcuts().length);
    expect(narrowed.every((one) => one.does.toLowerCase().includes('knowledge'))).toBe(true);
    expect(matchingShortcuts(shortcuts(), 'nothing by that name')).toHaveLength(0);
  });

  it('reaches the document, one drawn row per shortcut', () => {
    const markup = drawn('');
    expect(times(markup, 'data-shortcut=')).toBe(shortcuts().length);
    for (const one of shortcuts()) {
      expect(markup).toContain(one.does);
    }
    expect(markup).toContain('data-palette="shortcuts"');
    /* Droga powrotna do listy rzeczy do zrobienia — bez niej `?` jest ulicą jednokierunkową. */
    expect(markup).toContain('data-palette-show="items"');
  });

  it('shrinks the drawn list when a word is typed into the same field', () => {
    const markup = drawn('knowledge');
    expect(times(markup, 'data-shortcut=')).toBe(
      matchingShortcuts(shortcuts(), 'knowledge').length,
    );
    expect(times(markup, 'data-shortcut=')).toBeLessThan(shortcuts().length);
  });
});
