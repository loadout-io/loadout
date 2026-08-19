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

/** Przystanki kazdego gradientu w pliku, po identyfikatorze. */
function gradients(src: string): Map<string, readonly string[]> {
  const out = new Map<string, readonly string[]>();
  for (const hit of src.matchAll(
    /<(?:linear|radial)Gradient[^>]*\sid="([^"]+)"[^>]*>([\s\S]*?)<\/(?:linear|radial)Gradient>/g,
  )) {
    out.set(
      hit[1] ?? '',
      [...(hit[2] ?? '').matchAll(/stop-color="([^"]+)"/g)].map((stop) => stop[1] ?? ''),
    );
  }
  return out;
}

/** Barwy pod jednym odwolaniem: wprost albo przez gradient. */
function colours(value: string, defs: Map<string, readonly string[]>): readonly string[] {
  const link = /^url\(#(.+)\)$/.exec(value.trim());
  if (link !== null) return defs.get(link[1] ?? '') ?? [];
  return value === 'none' || value === '' ? [] : [value];
}

/** Barwy TEMATU: obrysy sciezek i wypelnienia malych okregow. */
function subjectColours(src: string): readonly string[] {
  const defs = gradients(src);
  const out: string[] = [];
  for (const hit of src.matchAll(/<(?:g|path)\b[^>]*\sstroke="([^"]+)"[^>]*>/g)) {
    out.push(...colours(hit[1] ?? '', defs));
  }
  for (const hit of src.matchAll(/<circle\b[^>]*\sr="([\d.]+)"[^>]*\sfill="([^"]+)"[^>]*>/g)) {
    if (Number(hit[1]) < NODE_LIMIT) out.push(...colours(hit[2] ?? '', defs));
  }
  return out;
}

/** Barwy TLA: wypelnienie pierwszego prostokata na cale plotno. */
function groundColours(src: string): readonly string[] {
  const defs = gradients(src);
  const hit = /<rect\b[^>]*\swidth="1024"[^>]*\sfill="([^"]+)"[^>]*>/.exec(src);
  return colours(hit?.[1] ?? '', defs);
}

/* AC-3 dla T-51: trzy rysunki mowia jedna paleta.
 *
 * Trzy osobne rysunki sa decyzja (T-49: przy 32 px krawedzie zlewaja sie w plame), ale osobny
 * rysunek nie znaczy osobna paleta. Zmierzone: rysunek 32 malowal krawedzie `#8f96ff`, ktorego
 * w rysunku pelnym nie ma, a rysunek 16 stal na tle `#161436`, ktorego tez tam nie ma. Ikona,
 * ktora zmienia barwe razem z rozmiarem, jest w Docku i na pasku menu dwiema roznymi ikonami.
 *
 * SLABA WERSJA: porownanie liczby barw. Trzy rysunki po trzy inne barwy przechodza, a to jest
 * dokladnie dzisiejszy stan.
 */

describe('jedna paleta', () => {
  const full = svg('loadout-icon.svg');
  const palette = new Set(subjectColours(full).map((one) => one.toLowerCase()));
  const ground = groundColours(full).map((one) => one.toLowerCase());

  it('reads a palette out of the full drawing at all', () => {
    expect(
      palette.size,
      'fewer than five subject colours were read out of the full drawing, so the comparison ' +
        'below is against an almost empty set',
    ).toBeGreaterThan(4);
    expect(ground.length, 'no ground colour was read out of the full drawing').toBeGreaterThan(2);
  });

  it('paints the smaller subjects out of the same palette', () => {
    for (const name of DRAWINGS.slice(1)) {
      const mine = subjectColours(svg(name)).map((one) => one.toLowerCase());
      expect(mine.length, 'no subject colour was read out of ' + name).toBeGreaterThan(0);
      const strangers = mine.filter((one) => !palette.has(one));
      expect(
        strangers,
        'these subject colours in ' +
          name +
          ' appear nowhere in the full drawing: ' +
          JSON.stringify(strangers) +
          '. An icon that changes colour with size is two icons.',
      ).toEqual([]);
    }
  });

  it('gives the 32 drawing the same ground as the full one', () => {
    expect(
      groundColours(svg('loadout-icon-32.svg')).map((one) => one.toLowerCase()),
      'the 32 px drawing stands on a different ground than the full one',
    ).toEqual(ground);
  });

  it('gives the 16 drawing one of those ground colours, flat', () => {
    const mine = groundColours(svg('loadout-icon-16.svg')).map((one) => one.toLowerCase());
    expect(
      mine.length,
      'the 16 px drawing has more than one ground colour. At that size a gradient is one pixel ' +
        'of grey: it costs bytes and changes nothing.',
    ).toBe(1);
    expect(
      ground,
      'the flat ground of the 16 px drawing is not one of the colours the tile is made of: ' +
        JSON.stringify(mine),
    ).toContain(mine[0]);
  });
});
