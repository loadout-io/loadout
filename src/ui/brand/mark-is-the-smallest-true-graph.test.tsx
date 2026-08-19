import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { Mark } from './mark';

/* AC-1 dla T-49: znak jest najmniejszym PRAWDZIWYM grafem, a geometria jest czytana z pliku.
 *
 * Jedno wejscie, dwie rownolegle galezie, jedna synteza — dwie z pieciu rzeczy z decyzji D6,
 * ktorych zaden vendor nie zbuduje, bo nie ma w tym interesu. Niezmiennik 17 zabrania rysowac
 * relacje, ktorych nie ma w danych, wiec marka bedaca najmniejszym grafem PRAWDZIWYM jest
 * jedynym mozliwym ornamentem tego produktu.
 *
 * SLABA WERSJA: asercja na obecnosc `<circle>`. Cztery luzne okregi bez krawedzi przechodza —
 * a to jest DOKLADNIE znak, ktory to zadanie zastepuje: cztery kwadraty obrocone o 45 stopni,
 * bez ani jednej krawedzi, wiec bez relacji, wiec nie graf.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const BRAND = resolve(ROOT, 'docs', 'branding');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');
const svg = (name: string): string => text(resolve(BRAND, name));

/** Liczba wezlow (okregow) w rysunku. */
const nodes = (src: string): number => [...src.matchAll(/<circle\b/g)].length;

/** Liczba krawedzi: polecen `L` w sciezkach. Przeniesienie piora (`M`) nie jest linia. */
const edges = (src: string): number => [...src.matchAll(/L\s*-?\d/g)].length;

/** Promienie wezlow w kolejnosci wystapienia. */
function radii(src: string): readonly number[] {
  return [...src.matchAll(/<circle[^>]*\sr="([\d.]+)"/g)].map((hit) => Number(hit[1]));
}

describe('znak jest najmniejszym prawdziwym grafem', () => {
  const mark = svg('loadout-mark.svg');

  it('exists at all', () => {
    expect(mark.length, 'docs/branding/loadout-mark.svg is missing or empty').toBeGreaterThan(100);
  });

  it('carries exactly four joining points', () => {
    expect(
      nodes(mark),
      'the mark has to carry four round marks: one input, two parallel branches, one synthesis. ' +
        'That is the smallest true statement of what this product does.',
    ).toBe(4);
  });

  it('carries exactly four edges, which is what makes it a graph at all', () => {
    expect(
      edges(mark),
      'the mark carries no four edges. Four loose round marks are a SET, not a graph — and that ' +
        'is exactly what the previous mark was: four squares turned 45 degrees with nothing ' +
        'joining them. The edges are the whole difference.',
    ).toBe(4);
  });

  it('makes the synthesis point the largest, because out of many comes one', () => {
    const all = radii(mark);
    expect(all.length, 'no radius was read out of the mark').toBe(4);
    const biggest = Math.max(...all);
    expect(
      all.filter((one) => one === biggest).length,
      'more than one round mark is the largest, so nothing says which one is the result. Out of ' +
        'many comes one, and the drawing has to say which.',
    ).toBe(1);
  });

  it('keeps the round marks readable against the line, at 3:1 or better', () => {
    const width = Number(/stroke-width="([\d.]+)"/.exec(mark)?.[1] ?? '0');
    expect(width, 'no stroke width was read out of the mark').toBeGreaterThan(0);
    const smallest = Math.min(...radii(mark));
    const ratio = (smallest * 2) / width;
    expect(
      ratio,
      'the round marks measure only ' +
        ratio.toFixed(2) +
        ' times the line. Below three they read as a THICKENING of the line rather than as ' +
        'separate points, and the whole mark closes into a ring — measured at 176 px.',
    ).toBeGreaterThanOrEqual(3);
  });

  it('is wider than tall, because the graph flows sideways', () => {
    const xs = [...mark.matchAll(/<circle[^>]*\scx="([\d.]+)"/g)].map((hit) => Number(hit[1]));
    const ys = [...mark.matchAll(/<circle[^>]*\scy="([\d.]+)"/g)].map((hit) => Number(hit[1]));
    expect(xs.length, 'no coordinates were read out of the mark').toBe(4);
    const wide = Math.max(...xs) - Math.min(...xs);
    const tall = Math.max(...ys) - Math.min(...ys);
    expect(
      wide,
      'the mark is not wider than tall (' +
        String(wide) +
        ' by ' +
        String(tall) +
        '). A symmetric diamond reads as a playing-card suit; the graph flows from an input to a ' +
        'result, and that direction is horizontal.',
    ).toBeGreaterThan(tall);
  });

  it('draws in code exactly what the drawing draws', () => {
    const rendered = renderToStaticMarkup(createElement(Mark, {}));
    expect(
      nodes(rendered),
      'the component renders a different number of round marks than the drawing holds, so the ' +
        'two have drifted and the drawing is no longer the source',
    ).toBe(nodes(mark));
    expect(
      edges(rendered),
      'the component renders a different number of edges than the drawing holds',
    ).toBe(edges(mark));
  });
});
