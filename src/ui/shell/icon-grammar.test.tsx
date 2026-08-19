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

/** Ile okregow niesie glif. */
function circles(svg: string): number {
  return [...svg.matchAll(/<circle\b/g)].length;
}

/** Ile prostych odcinkow niesie glif: `<line>` albo polecenie `L` w sciezce. */
function segments(svg: string): number {
  const lines = [...svg.matchAll(/<line\b/g)].length;
  const paths = [...svg.matchAll(/\sd=/g)].length;
  const moves = [...svg.matchAll(/[ML]\s*-?\d/g)].length;
  return lines + (paths > 0 ? moves : 0);
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
      circles(svg),
      'the workflow glyph carries no round marks. It is the one section whose subject is a graph, ' +
        'so it is the one glyph allowed round marks joined by lines.',
    ).toBeGreaterThan(1);
    expect(
      segments(svg),
      'the workflow glyph carries round marks and nothing joining them, so it draws a set and ' +
        'not a graph — and the joining lines are the whole difference between the two.',
    ).toBeGreaterThan(0);
  });

  it('draws sets as sets, with no edge joining anything', () => {
    const sets = ['agents', 'skills', 'memory'];
    const guilty = sets.filter((id) => circles(glyph(id)) > 1 && segments(glyph(id)) > 0);
    expect(
      guilty,
      'these glyphs join shapes with an edge and their subject is a set, not a graph. An icon ' +
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
