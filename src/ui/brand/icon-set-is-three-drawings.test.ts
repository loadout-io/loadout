import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-2 dla T-49: ikona to TRZY rysunki, nie jeden przeskalowany.
 *
 * Przy 32 px cztery krawedzie rysunku pelnego (38 na 1024) mierza niecale 1,2 px i zlewaja sie
 * w plame; sheen i krawedz wewnetrzna operuja na 3/1024, czyli na 0,1 px, i po prostu znikaja.
 * `.icns` jest ZESTAWEM, a ikona, ktora mydli sie na pasku Docka, jest pierwsza rzecza, jaka
 * czlowiek widzi o jakosci aplikacji.
 *
 * SLABA WERSJA: sprawdzenie, ze trzy pliki istnieja. Trzy kopie tego samego rysunku przechodza,
 * a caly sens tego kryterium jest w tym, ze sie ROZNIA.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const BRAND = resolve(ROOT, 'docs', 'branding');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');
const svg = (name: string): string => text(resolve(BRAND, name));

/* WSZYSTKIE grubosci krawedzi w proporcji do plotna, nie pierwsza.
 *
 * Do 2026-08-19 czytana byla tylko PIERWSZA — a wtedy bajtowa kopia rysunku pelnego z jedna
 * podmieniona liczba przechodzila cale to kryterium, zachowujac poswiate, gradienty wezlow
 * i krawedz wewnetrzna, czyli dokladnie roznice ZADEKLAROWANA zamiast prawdziwej. Pelny rysunek
 * niesie dwie krawedzie: temat (38) i wlos wewnetrzny (3). Kiedy warunek brzmi „KAZDA krawedz
 * mniejszego rysunku jest grubsza od najgrubszej krawedzi pelnego", taka kopia pada na tym
 * wlosie, bo wlos zostaje. */
function strokeShares(src: string): readonly number[] {
  const canvas = Number(/viewBox="0 0 (\d+)/.exec(src)?.[1] ?? '0');
  if (canvas === 0) return [];
  return [...src.matchAll(/stroke-width="([\d.]+)"/g)].map((hit) => Number(hit[1]) / canvas);
}

/** Najgrubsza krawedz rysunku, w proporcji do plotna. */
const thickest = (src: string): number => Math.max(0, ...strokeShares(src));

/** Promien squircle w proporcji do plotna. */
function cornerShare(src: string): number {
  const rx = Number(/rx="([\d.]+)"/.exec(src)?.[1] ?? '0');
  const canvas = Number(/viewBox="0 0 (\d+)/.exec(src)?.[1] ?? '0');
  return canvas === 0 ? 0 : rx / canvas;
}

const gradients = (src: string): number =>
  [...src.matchAll(/<(?:radial|linear)Gradient\b/g)].length;

/* Podpis poswiaty i blasku, czytany BEZ nazwy: przejscie w nic.
 *
 * Sprawdzanie `id="sheen"` mierzy identyfikator, ktory kazdy moze przezwac jednym ruchem —
 * a przejscia w przezroczystosc przezwac nie da sie, bo to one rysuja efekt. */
const fades = (src: string): number => [...src.matchAll(/stop-opacity="0(?:\.0+)?"/g)].length;

/** Krawedz wewnetrzna: obrys o wlasnej przezroczystosci. */
const innerEdge = (src: string): boolean => /stroke-opacity="/.test(src);

describe('zestaw ikony', () => {
  const full = svg('loadout-icon.svg');
  const small = svg('loadout-icon-32.svg');
  const tiny = svg('loadout-icon-16.svg');

  it('has all three drawings', () => {
    for (const [name, src] of [
      ['loadout-icon.svg', full],
      ['loadout-icon-32.svg', small],
      ['loadout-icon-16.svg', tiny],
    ] as const) {
      expect(src.length, name + ' is missing or empty').toBeGreaterThan(100);
    }
  });

  it('draws the smallest one in ONE colour, with no gradient at all', () => {
    expect(
      gradients(tiny),
      'the 16 px drawing carries a gradient. At that size every gradient is one pixel of grey: ' +
        'it costs bytes, changes nothing, and makes the shape muddier than a flat fill.',
    ).toBe(0);
  });

  it('thickens EVERY line as the drawing gets smaller', () => {
    expect(strokeShares(full).length, 'no stroke was read out of the full drawing').toBeGreaterThan(
      1,
    );
    for (const [name, src] of [
      ['loadout-icon-32.svg', small],
      ['loadout-icon-16.svg', tiny],
    ] as const) {
      const shares = strokeShares(src);
      expect(shares.length, 'no stroke was read out of ' + name).toBeGreaterThan(0);
      expect(
        Math.min(...shares),
        name +
          ' carries a line no thicker than the full drawing carries. At 32 px a line of that ' +
          'weight measures barely one pixel and the four edges merge into a blob — which is the ' +
          'whole reason this file exists separately. A byte copy of the full drawing with one ' +
          'number changed fails here, because the hairline it copied along stays.',
      ).toBeGreaterThan(thickest(full));
    }
    expect(
      thickest(tiny),
      'the 16 px drawing does not thicken its line beyond the 32 px one',
    ).toBeGreaterThan(thickest(small));
  });

  it('keeps the SAME corner on all three, so the icon does not change shape with size', () => {
    const shares = [cornerShare(full), cornerShare(small), cornerShare(tiny)];
    expect(
      shares.every((one) => one > 0),
      'no corner radius was read out of one of the drawings',
    ).toBe(true);
    const spread = Math.max(...shares) - Math.min(...shares);
    expect(
      spread,
      'the three drawings round their corner differently (' +
        JSON.stringify(shares.map((one) => one.toFixed(4))) +
        '). An icon that changes shape with size reads as three different apps in three places ' +
        'of the same system.',
    ).toBeLessThan(0.0005);
  });

  it('keeps the gleam and the inner edge for the full drawing only', () => {
    expect(
      fades(full),
      'the full drawing carries no fade into nothing, so it lost the gleam and the glow. They ' +
        'are two of the three things that make this icon read as a sibling of the app next door.',
    ).toBeGreaterThanOrEqual(2);
    expect(gradients(full), 'the full drawing lost its gradients').toBeGreaterThanOrEqual(4);
    expect(innerEdge(full), 'the full drawing lost its crisp inner edge').toBe(true);

    for (const [name, src, allowed] of [
      ['loadout-icon-32.svg', small, 1],
      ['loadout-icon-16.svg', tiny, 0],
    ] as const) {
      expect(
        fades(src),
        name +
          ' fades something into nothing. At its size such a fade operates on a tenth of a ' +
          'pixel, so keeping it means the difference between these drawings is declared rather ' +
          'than real — and renaming the fade changes nothing about that, which is why this is ' +
          'measured on the fade itself and not on what it is called.',
      ).toBe(0);
      expect(
        innerEdge(src),
        name + ' keeps the inner edge, which at its size is a fraction of a pixel of grey',
      ).toBe(false);
      expect(
        gradients(src),
        name +
          ' carries more gradients than the background alone needs, so it is the full drawing ' +
          'wearing a different first number rather than its own drawing',
      ).toBeLessThanOrEqual(allowed);
    }
  });
});
