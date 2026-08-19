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

/* CALA geometria jednym napisem — i ta sama funkcja czyta rysunek oraz wyrenderowany komponent.
 *
 * Do 2026-08-19 kod byl wiazany z rysunkiem wylacznie LICZBA wezlow i krawedzi, a wszystkie trzy
 * asercje o ksztalcie (synteza najwieksza, stosunek 3 : 1, szerszy niz wysoki) mierzyly TYLKO
 * plik, ktorego nikt nie renderuje. `Mark` z czterema rownymi promieniami, krawedzia 4 i sylwetka
 * wyzsza niz szersza przechodzil wtedy caly ten plik — czyli w aplikacji stal dokladnie ten
 * pierscien, ktoremu to kryterium ma zabraniac. Numery zyja w dwoch miejscach (niezmiennik 13),
 * bo komponent nie ma jak przeczytac pliku z `docs/` w czasie biegu; skoro tak, to porownanie
 * musi byc WARTOSC ZA WARTOSC, a nie zgodnoscia dwoch licznikow. */
function geometry(src: string): string {
  const path = (/\sd="([^"]+)"/.exec(src)?.[1] ?? '').replace(/\s+/g, ' ').trim();
  const points = [
    ...src.matchAll(/<circle[^>]*\scx="([\d.]+)"[^>]*\scy="([\d.]+)"[^>]*\sr="([\d.]+)"/g),
  ].map((hit) => hit.slice(1, 4).join(','));
  const width = /stroke-width="([\d.]+)"/.exec(src)?.[1] ?? '';
  return [path, points.join(' '), width].join(' | ');
}

describe('znak jest najmniejszym prawdziwym grafem', () => {
  const mark = svg('loadout-mark.svg');
  const rendered = renderToStaticMarkup(createElement(Mark, {}));

  /** Kazda asercja o ksztalcie biegnie po OBU zrodlach: po rysunku i po tym, co widzi czlowiek. */
  const both = [
    ['the drawing', mark],
    ['the component', rendered],
  ] as const;

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
    for (const [where, src] of both) {
      const all = radii(src);
      expect(all.length, 'no radius was read out of ' + where).toBe(4);
      const biggest = Math.max(...all);
      expect(
        all.filter((one) => one === biggest).length,
        'in ' +
          where +
          ', more than one round mark is the largest, so nothing says which one is the result. ' +
          'Out of many comes one, and the mark has to say which.',
      ).toBe(1);
    }
  });

  it('keeps the round marks readable against the line, at 3:1 or better', () => {
    for (const [where, src] of both) {
      const width = Number(/stroke-width="([\d.]+)"/.exec(src)?.[1] ?? '0');
      expect(width, 'no stroke width was read out of ' + where).toBeGreaterThan(0);
      const smallest = Math.min(...radii(src));
      const ratio = (smallest * 2) / width;
      expect(
        ratio,
        'in ' +
          where +
          ', the round marks measure only ' +
          ratio.toFixed(2) +
          ' times the line. Below three they read as a THICKENING of the line rather than as ' +
          'separate points, and the whole mark closes into a ring — measured at 176 px.',
      ).toBeGreaterThanOrEqual(3);
    }
  });

  it('is wider than tall, because the graph flows sideways', () => {
    for (const [where, src] of both) {
      const xs = [...src.matchAll(/<circle[^>]*\scx="([\d.]+)"/g)].map((hit) => Number(hit[1]));
      const ys = [...src.matchAll(/<circle[^>]*\scy="([\d.]+)"/g)].map((hit) => Number(hit[1]));
      expect(xs.length, 'no coordinates were read out of ' + where).toBe(4);
      const wide = Math.max(...xs) - Math.min(...xs);
      const tall = Math.max(...ys) - Math.min(...ys);
      expect(
        wide,
        where +
          ' is not wider than tall (' +
          String(wide) +
          ' by ' +
          String(tall) +
          '). A symmetric diamond reads as a playing-card suit; the graph flows from an input to ' +
          'a result, and that direction is horizontal.',
      ).toBeGreaterThan(tall);
    }
  });

  it('draws in code exactly what the drawing draws, value for value', () => {
    expect(
      nodes(rendered),
      'the component renders a different number of round marks than the drawing holds, so the ' +
        'two have drifted and the drawing is no longer the source',
    ).toBe(nodes(mark));
    expect(
      edges(rendered),
      'the component renders a different number of edges than the drawing holds',
    ).toBe(edges(mark));

    /* Kontrola przeciw pustce: gdyby `geometry` przestala cokolwiek czytac, dwie puste wartosci
     * byly by sobie rowne i ta asercja przechodzilaby na kazdym kodzie. */
    const shape = geometry(mark);
    expect(shape.length, 'no geometry at all was read out of the drawing').toBeGreaterThan(60);
    expect(shape.split('|').length, 'the geometry was read without its three parts').toBe(3);
    expect(
      geometry(rendered),
      'the component and the drawing agree on how MANY points and edges there are, and disagree ' +
        'on where they sit. Two hand-kept copies of the same numbers is one fact in two places ' +
        '(invariant 13); a matching pair of counts is not a matching mark.',
    ).toBe(shape);
  });
});
