import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { SECTIONS } from '../sections';

/* AC-4 dla T-46: akcent bierze GLIF, nigdy tla aktywnego wiersza.
 *
 * To regula domu, wprost z `../meetnotes/src/design-tokens/glass.css`: „the accent never fills
 * chrome, it colors the active glyph/label only". Powloka, w ktorej akcentem swieci tlo, etykieta
 * i glif naraz, mowi trzy razy jedna rzecz — a `--accent` znaczy „to jest interaktywne", nie
 * „tu jestes".
 *
 * Ktora sekcja jest otwarta, jest powiedziane DOKLADNIE RAZ, przez `aria-current` (niezmiennik 13).
 * Wyglad bierze sie z tego samego atrybutu, a nie z drugiej kopii tej prawdy w klasie.
 */

const markup = renderToStaticMarkup(createElement(App, { section: 'run', screens: {} }));

/* ZAKRES: wylacznie PRZELACZNIKI SEKCJI. W `<nav>` stoi tez przelacznik zakresu
 * (`workspace-switcher`), ktory sekcja nie jest — zliczony razem z nimi dawal szesc kontrolek
 * przy pieciu sekcjach i punkt o liczbie padal na wlasnym parserze, nie na kodzie. */
function sectionSwitches(html: string): readonly string[] {
  const nav = /<nav[\s\S]*?<\/nav>/.exec(html)?.[0] ?? '';
  return [...nav.matchAll(/<button[^>]*data-section-switch[\s\S]*?<\/button>/g)].map(
    (hit) => hit[0],
  );
}

/** Znacznik otwierajacy z bloku przycisku. */
function openingTag(block: string): string {
  return /<button[^>]*>/.exec(block)?.[0] ?? '';
}

const OPEN = /aria-current=/;
/* Flaga `g` jest wymogiem `matchAll`, nie ozdoba: bez niej rzuca TypeError, a punkt
 * o liczeniu wystapien nie sadzi niczego, tylko sie wywala. */
const ACCENT = /\baccent\b/g;
/* Akcent BRAMKOWANY: wystepuje wylacznie jako wariant `aria-[current=true]`. */
const GATED = /aria-\[current=true\]:[a-z-]*accent\b/g;

describe('akcent bierze glif', () => {
  const blocks = sectionSwitches(markup);
  const buttons = blocks.map(openingTag);

  it('renders one navigation control per section, counted from the registry', () => {
    expect(
      blocks.length,
      'the shell rendered a different number of section switches than there are sections. ' +
        'With none of them rendered every point below would pass on an empty list.',
    ).toBe(SECTIONS.length);
    expect(
      buttons.filter((tag) => tag !== '').length,
      'a section switch body was read but its opening tag was not',
    ).toBe(SECTIONS.length);
  });

  it('says which section is open exactly once, through aria-current', () => {
    const open = buttons.filter((tag) => OPEN.test(tag));
    expect(
      open.length,
      'exactly one navigation control has to carry aria-current. Zero means the shell never ' +
        'says where you are; two mean it says it twice and one of them is wrong.',
    ).toBe(1);
  });

  it('keeps the accent OUT of the control box itself', () => {
    const guilty = buttons.filter((tag) => [...tag.matchAll(ACCENT)].length > 0);
    expect(
      guilty,
      'a navigation control carries the accent on its own box. The accent means "this is ' +
        'interactive", not "you are here" — and chrome filled with it stops being chrome.',
    ).toEqual([]);
  });

  it('puts the accent on the glyph of the active control, exactly once', () => {
    const openBlock = blocks.find((block) => OPEN.test(openingTag(block))) ?? '';
    expect(openBlock, 'no active navigation control body could be read').not.toBe('');
    const hits = [...openBlock.matchAll(ACCENT)].length;
    expect(
      hits,
      'the active control mentions the accent ' +
        String(hits) +
        ' time(s) inside its body. It has to be exactly one — the glyph — because two places for ' +
        'one fact is the failure invariant 13 names.',
    ).toBe(1);
  });

  it('lets the accent exist only as something derived from aria-current', () => {
    /* POPRAWIONE po pierwszym biegu. Punkt brzmial „wiersze nieaktywne nie niosa akcentu
     * wcale" i byl NIESPELNIALNY przez poprawny kod: w statycznym markupie klasa wariantowa
     * stoi na KAZDYM przycisku niezaleznie od stanu, wiec zeby ja z nieaktywnych usunac,
     * trzeba policzyc aktywnosc drugi raz w TSX — czyli zrobic druga kopie decyzji, ktorej
     * zabrania niezmiennik 13. Ta forma jest mocniejsza: barwa nie ma prawa istniec inaczej
     * niz jako pochodna jedynego zrodla prawdy o tym, gdzie jestes. */
    const bare: string[] = [];
    for (const block of blocks) {
      const all = [...block.matchAll(ACCENT)].length;
      const gated = [...block.matchAll(GATED)].length;
      if (all !== gated) bare.push(block.slice(0, 120));
    }
    expect(
      bare,
      'these navigation controls mention the accent outside a variant of aria-current, so the ' +
        'colour is stated a second time instead of derived from the one place that knows which ' +
        'section is open',
    ).toEqual([]);
  });
});
