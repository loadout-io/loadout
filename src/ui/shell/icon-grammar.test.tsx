import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { SECTIONS } from '../sections';
import { NavIcon } from './nav-icons';

/* AC-5 dla T-46: piec glifow, i gramatyka ikon jest SPRAWDZALNA.
 *
 * Wezly i krawedzie WYLACZNIE dla rzeczy, ktore sa grafem. To niezmiennik 17 przeniesiony na
 * ikonografie: nie rysujemy relacji tam, gdzie relacji nie ma. Workflow JEST grafem, wiec jego
 * glif ma wezly i krawedzie. Agenci, umiejetnosci i pamiec sa ZBIORAMI, wiec ich glify maja
 * plyty i ani jednej linii lączącej — inaczej ikona obiecuje zaleznosc, ktorej w danych nie ma.
 *
 * SLABA WERSJA: policzenie piatki. Piec pustych `<svg>` przechodzi.
 */

function glyph(section: string): string {
  return renderToStaticMarkup(createElement(NavIcon, { section }));
}

/** Ile okregow niesie glif. Okrag to WEZEL. */
function nodeMarks(svg: string): number {
  return [...svg.matchAll(/<circle\b/g)].length;
}

/**
 * Ile KRAWEDZI niesie glif: element `<line>` albo polecenie `L` w sciezce.
 *
 * POPRAWIONE po drugiej opinii 2026-08-19. Poprzednia wersja liczyla takze `M`, czyli samo
 * PRZENIESIENIE piora — a graf, ktorego sciezka ma wylacznie `M`, nie rysuje ani jednej
 * krawedzi i przechodzil punkt o „wezlach polaczonych liniami". Przeniesienie nie jest linia.
 */
function edgeMarks(svg: string): number {
  return [...svg.matchAll(/<line\b/g)].length + [...svg.matchAll(/L\s*-?\d/g)].length;
}

describe('gramatyka ikon nawigacji', () => {
  it('has a glyph for every section, counted from the registry', () => {
    const missing = SECTIONS.filter((entry) => glyph(entry.id).trim() === '').map(
      (entry) => entry.id,
    );
    expect(
      missing,
      'these sections render no glyph at all. With an empty glyph every point below would be ' +
        'measuring an empty string.',
    ).toEqual([]);
    expect(SECTIONS.length, 'the section registry is empty').toBeGreaterThan(0);
  });

  it('draws the workflow glyph as a graph, because a workflow IS one', () => {
    const svg = glyph('workflows');
    expect(
      nodeMarks(svg),
      'the workflow glyph carries no round marks. It is the one section whose subject is a graph, ' +
        'so it is the one glyph allowed round marks joined by lines.',
    ).toBeGreaterThan(1);
    expect(
      edgeMarks(svg),
      'the workflow glyph carries round marks and nothing joining them, so it draws a set and ' +
        'not a graph — and the joining lines are the whole difference between the two.',
    ).toBeGreaterThan(0);
  });

  it('draws sets as sets: no round marks and nothing joining anything', () => {
    /* POPRAWIONE po drugiej opinii 2026-08-19. Poprzedni warunek brzmial
     * `circles > 1 && segments > 0`, a zaden glif zbioru nie ma ANI JEDNEGO okregu — czyli
     * pierwszy czlon byl zawsze falszywy i punkt nie sadzil zadnego glifu. Najlenieszy
     * przechodzacy glif zbioru to dwie plyty spiete jawnym `<line>`: zero okregow, wiec nigdy
     * nie zgloszony, a ikona obiecuje zaleznosc miedzy agentami, ktorej w danych nie ma.
     *
     * Teraz warunek jest ROZLACZNY i sprawdza kazdy glif zbioru osobno: zbior nie ma prawa
     * niesc wezla (okregu) ani krawedzi (`<line>`). Gwiazda w `skills` jest jednym obrysem
     * wielokata — ma polecenia `L`, ale ani jednego `<line>` i ani jednego okregu — wiec
     * przechodzi, i to jest poprawne: wielokat nie lączy dwoch rzeczy. */
    const sets = ['agents', 'skills', 'memory'];
    const withNodes = sets.filter((id) => nodeMarks(glyph(id)) > 0);
    expect(
      withNodes,
      'these glyphs carry round marks and their subject is a set, not a graph. A round mark is ' +
        'a joining point, and those exist in this alphabet only where a relation really does.',
    ).toEqual([]);
    const joined = sets.filter((id) => /<line\b/.test(glyph(id)));
    expect(
      joined,
      'these glyphs join two shapes with an explicit line and their subject is a set. An icon ' +
        'that draws a relation the data does not have is the same failure as a canvas drawing ' +
        'a curve between hard-coded points (invariant 17).',
    ).toEqual([]);
  });

  it('takes its colour from the text, never from a value of its own', () => {
    const guilty = SECTIONS.filter((entry) =>
      /#[0-9a-fA-F]{3,8}\b|rgba?\(/.test(glyph(entry.id)),
    ).map((entry) => entry.id);
    expect(
      guilty,
      'these glyphs state a colour instead of inheriting it. The active one has to take the ' +
        'accent and the rest the muted text colour, and one glyph holding its own value cannot ' +
        'do both.',
    ).toEqual([]);
  });

  it('hides every glyph from a screen reader, because the label is right beside it', () => {
    const guilty = SECTIONS.filter((entry) => !/aria-hidden/.test(glyph(entry.id))).map(
      (entry) => entry.id,
    );
    expect(
      guilty,
      'these glyphs are not hidden from assistive technology. The label stands next to the ' +
        'glyph, so a second name for the same thing is noise, not help.',
    ).toEqual([]);
  });
});
