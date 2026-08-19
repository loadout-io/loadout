import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const BRAND = resolve(ROOT, 'docs', 'branding');
const DRAWINGS = ['loadout-icon.svg', 'loadout-icon-32.svg', 'loadout-icon-16.svg'] as const;

const svg = (name: string): string => {
  const path = resolve(BRAND, name);
  return existsSync(path) ? readFileSync(path, 'utf8').replace(/<!--[\s\S]*?-->/g, ' ') : '';
};

/* CZYTNIK JEST W KAZDYM Z CZTERECH PLIKOW OSOBNO, i to jest celowe: kryterium ma dac sie
 * przeczytac i uruchomic samo, bez skakania do wspolnego modulu. Kazdy czyta tylko to, o co
 * pyta jego wlasna asercja.
 *
 * TEMAT TO WEZLY I KRAWEDZIE, nie wszystko, co jest w pliku. Sheen i krawedz wewnetrzna sa
 * `<rect>`-ami i wolno im niesc biel, bo dzialaja przy 10% i 22%. Blask jest `<circle>`, ale
 * o promieniu 32% plotna — dlatego wezel poznaje sie po promieniu MNIEJSZYM niz 15% plotna,
 * a nie po nazwie gradientu, ktora kazdy moze zmienic. */
const CANVAS = 1024;
const NODE_LIMIT = CANVAS * 0.15;

/* AC-2 dla T-51: temat wypelnia kafle, a nie plywa w niej.
 *
 * Zmierzone na wyladowanym rysunku: temat zajmowal 66% szerokosci i 39% wysokosci plotna. Ikona
 * w Docku jest ogladana przy 32-64 px i sasiaduje z ikonami, ktorych temat wypelnia kafle prawie
 * w calosci; przy dwoch trzecich zostaje duze puste pole, a znak przestaje byc rozpoznawalny
 * z odleglosci reki.
 *
 * SLABA WERSJA: asercja na jeden rysunek. Rysunek 32 ma dzis 68% i przechodzi progi ustawione
 * na rysunek pelny.
 */

const WIDE = 0.7;
const TALL = 0.42;
const MARGIN = 0.08;
const OFF_CENTRE = 0.02;

/** Wezly z promieniami: cx, cy, r. Blask ma promien 32% plotna i tu nie wchodzi. */
function nodes(src: string): readonly (readonly [number, number, number])[] {
  return [...src.matchAll(/<circle\b[^>]*\scx="([\d.]+)"[^>]*\scy="([\d.]+)"[^>]*\sr="([\d.]+)"/g)]
    .map((hit) => [Number(hit[1]), Number(hit[2]), Number(hit[3])] as const)
    .filter(([, , r]) => r < NODE_LIMIT);
}

/** Konce krawedzi: kazde `M x y` i `L x y` ze sciezek. */
function ends(src: string): readonly (readonly [number, number])[] {
  return [...src.matchAll(/[ML]\s*(-?[\d.]+)\s+(-?[\d.]+)/g)].map(
    (hit) => [Number(hit[1]), Number(hit[2])] as const,
  );
}

describe('zasieg tematu', () => {
  const read = DRAWINGS.map((name) => [name, nodes(svg(name))] as const);

  it('read four joining points out of every drawing', () => {
    for (const [name, list] of read) {
      expect(
        list.length,
        'fewer than four joining points were read out of ' +
          name +
          ', so the extent below is measured on a fragment',
      ).toBe(4);
    }
  });

  it('fills the tile in both directions', () => {
    for (const [name, list] of read) {
      const left = Math.min(...list.map(([x, , r]) => x - r));
      const right = Math.max(...list.map(([x, , r]) => x + r));
      const top = Math.min(...list.map(([, y, r]) => y - r));
      const bottom = Math.max(...list.map(([, y, r]) => y + r));
      const wide = (right - left) / CANVAS;
      const tall = (bottom - top) / CANVAS;
      expect(
        wide,
        name +
          ' gives its subject ' +
          (wide * 100).toFixed(0) +
          '% of the width. An icon is read at 32 px beside icons whose subject nearly fills the ' +
          'tile; two thirds leaves a field of empty ground and the shape stops being recognisable.',
      ).toBeGreaterThanOrEqual(WIDE);
      expect(
        tall,
        name + ' gives its subject only ' + (tall * 100).toFixed(0) + '% of the height',
      ).toBeGreaterThanOrEqual(TALL);
    }
  });

  /* KRAWEDZIE SA CZESCIA TEMATU. Zmierzone kontrola negatywna 2026-08-19: zasieg liczony z samych
   * wezlow nie widzi sciezki, ktora do wezla nie dochodzi. Cztery kropki i cztery linie obok nich
   * to nie graf, tylko rozjechany rysunek — a przy 32 px wyglada jak brud. */
  it('runs every edge from one joining point to another', () => {
    for (const name of DRAWINGS) {
      const src = svg(name);
      const points = nodes(src);
      const stops = ends(src);
      expect(stops.length, 'no edge end was read out of ' + name).toBe(8);
      const orphans = stops.filter(
        ([x, y]) => !points.some(([cx, cy]) => Math.hypot(cx - x, cy - y) < 1),
      );
      expect(
        orphans,
        'these edge ends in ' +
          name +
          ' stop at no joining point: ' +
          JSON.stringify(orphans) +
          '. Four dots with four lines beside them is not a graph, it is a drawing that has come ' +
          'apart — and at 32 px it reads as dirt.',
      ).toEqual([]);
    }
  });

  it('keeps the subject centred and off the edge', () => {
    for (const [name, list] of read) {
      const left = Math.min(...list.map(([x, , r]) => x - r));
      const right = Math.max(...list.map(([x, , r]) => x + r));
      const top = Math.min(...list.map(([, y, r]) => y - r));
      const bottom = Math.max(...list.map(([, y, r]) => y + r));
      expect(
        Math.abs((left + right) / 2 - CANVAS / 2) / CANVAS,
        name + ' hangs its subject off centre sideways',
      ).toBeLessThan(OFF_CENTRE);
      expect(
        Math.abs((top + bottom) / 2 - CANVAS / 2) / CANVAS,
        name + ' hangs its subject off centre vertically',
      ).toBeLessThan(OFF_CENTRE);
      expect(
        Math.min(left, top, CANVAS - right, CANVAS - bottom) / CANVAS,
        name +
          ' pushes its subject to the edge of the tile. The squircle cuts the corners, so a ' +
          'subject touching the edge reads as cropped.',
      ).toBeGreaterThanOrEqual(MARGIN);
    }
  });
});
