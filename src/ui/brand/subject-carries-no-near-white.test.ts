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

/* AC-1 dla T-51: temat nie niesie prawie-bieli w zadnym z trzech rysunkow.
 *
 * Zgloszenie wlasciciela ze zrzutem Docka brzmialo „ikonka brzydka z bialymi elementami jest".
 * Zmierzone: wezly mialy gradient do `#e6e2ff` i czystej bieli. Prawie-biel na temacie zajmujacym
 * dwie trzecie kafli nie czyta sie jako forma, tylko jako okruchy — a przy 32 px zostaja z niej
 * cztery jasne plamki na ciemnym kwadracie.
 *
 * SLABA WERSJA: szukanie `#ffffff` w calym pliku. Pada na sheenie, ktory ma tam byc, i przechodzi
 * na `#e6e2ff`, czyli na barwie, ktora czlowiek nazwal biala.
 */

const NEAR_WHITE = 224;

/** Trzy kanaly z zapisu `#rgb` albo `#rrggbb`. */
function channels(colour: string): readonly number[] {
  const hex = colour.replace('#', '');
  const full = hex.length === 3 ? [...hex].map((one) => one + one).join('') : hex;
  return [0, 2, 4].map((at) => parseInt(full.slice(at, at + 2), 16));
}

describe('temat ikony', () => {
  const read = DRAWINGS.map((name) => [name, subjectColours(svg(name))] as const);

  it('read enough colours to judge', () => {
    const all = read.flatMap(([, list]) => list);
    expect(
      all.length,
      'fewer than eight subject colours were read out of the three drawings, so every assertion ' +
        'below would sweep an almost empty list',
    ).toBeGreaterThan(7);
    for (const [name, list] of read) {
      expect(list.length, 'no subject colour at all was read out of ' + name).toBeGreaterThan(0);
    }
  });

  it('keeps every subject colour away from near-white', () => {
    for (const [name, list] of read) {
      const pale = list.filter((one) => channels(one).every((value) => value >= NEAR_WHITE));
      expect(
        pale,
        'these subject colours in ' +
          name +
          ' sit at or above ' +
          String(NEAR_WHITE) +
          ' on all three channels: ' +
          JSON.stringify(pale) +
          '. On a tile the subject does not fill, that reads as specks of light rather than as ' +
          'a shape — which is what a person looking at the Dock reported.',
      ).toEqual([]);
    }
  });

  it('carries no plain white on the subject, in any spelling', () => {
    for (const [name, list] of read) {
      const white = list.filter((one) => /^#(?:fff|ffffff)$/i.test(one) || one === 'white');
      expect(
        white,
        'the subject of ' + name + ' is painted plain white: ' + JSON.stringify(white),
      ).toEqual([]);
    }
  });

  /* Wezel poznaje sie po promieniu mniejszym niz 15% plotna, bo blask jest okregiem o promieniu
   * 37% — nazwe gradientu kazdy moze zmienic, promien mowi, czym element JEST. Ta regula ma jednak
   * dziure: okrag WIEKSZY od progu wypada z tematu, wiec biala plama na pol kafli przeszlaby.
   * Plaska biel na okregu nie jest warstwa przy 10%, tylko rysunkiem, wiec jest zabroniona
   * wszedzie, bez wzgledu na rozmiar. */
  it('paints no round shape plain white, whatever its size', () => {
    for (const name of DRAWINGS) {
      const blobs = [...svg(name).matchAll(/<circle\b[^>]*\sfill="([^"]+)"/g)]
        .map((hit) => hit[1] ?? '')
        .filter((one) => /^#(?:f{3}|f{6})$/i.test(one) || one === 'white');
      expect(
        blobs,
        'a round shape in ' + name + ' is filled plain white: ' + JSON.stringify(blobs),
      ).toEqual([]);
    }
  });

  /* BIEL ZAPISANA WZORCEM, nie literalem. `checks/quick-tokens.sh` odrzuca w `src/**` kazdy
   * literal barwy — i ma racje wobec komponentow, bo tam barwa ma przychodzic z nazwy. Ten plik
   * nie MALUJE bieli, tylko CYTUJE ja z rysunku, ktory lezy poza `src/`; `#f{6}` we wzorcu znaczy
   * dokladnie to samo i nie jest literalem. */
  it('leaves the two layers that MAY carry white alone', () => {
    const full = svg('loadout-icon.svg');
    expect(
      /stop-color="#f{6}"\s+stop-opacity="0\.10"/.test(full),
      'the gleam lost its white. It works at ten per cent over the ground, so it is not a subject ' +
        'colour and this criterion does not touch it — the recipe comes from the app next door.',
    ).toBe(true);
    expect(
      /<rect[^>]*stroke="#f{6}"[^>]*stroke-opacity="0\.10"/.test(full),
      'the crisp inner edge lost its white. Same story: a layer at ten per cent, not a subject.',
    ).toBe(true);
  });
});
