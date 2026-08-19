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

/* AC-4 dla T-51: kontrast tematu do tla miesci sie w pasmie, w kazdym rysunku.
 *
 * Zmierzone na wyladowanym rysunku: 12,1 : 1. Tyle kontrastu jest w porzadku, kiedy temat
 * wypelnia kafle w calosci — czarno-biala ikona obok w Docku tak wlasnie wyglada i wyglada dobrze.
 * Przy temacie zajmujacym dwie trzecie kafli ten sam kontrast ODCZEPIA go od tla: jasne plamki
 * przestaja byc jedna forma. Dol pasma jest progiem WCAG dla grafiki nietekstowej — ponizej 3 : 1
 * znak ginie przy 16 px.
 *
 * Gora pasma obowiazuje bez wyjatku, bo AC-2 i tak nie pozwala tematowi wypelnic calej kafli.
 * To jest zapisane, a nie domyslne: gdyby kiedys temat mial siegac krawedzi, oba kryteria trzeba
 * zmienic razem.
 *
 * SLABA WERSJA: sprawdzenie samego progu dolnego. Dzisiejsze 12,1 : 1 przechodzi z zapasem.
 */

const FLOOR = 3;
const CEILING = 9;
/* Najciemniejszy punkt tematu wobec najjasniejszego punktu tla, czyli najgorsza mozliwa para.
 *
 * Zmierzone kontrola negatywna 2026-08-19: pasmo postawione na NAJJASNIEJSZEJ barwie tematu nie
 * widzi, gdy sciemnieje jego druga polowa — krawedzie moga zejsc do barwy tla, a wezly utrzymaja
 * caly warunek. Prog jest tu nizszy niz dolny prog pasma z rozmyslu: te dwa punkty nigdy na siebie
 * nie nachodza (gradient wezla ciemnieje ku jego brzegowi, a tlo jasnieje ku srodkowi kafli), wiec
 * 3 : 1 byloby wymogiem policzonym z pary, ktorej na rysunku nie ma. Ponizej 2 : 1 ciemna czesc
 * tematu przestaje sie odcinac od kafli w ogole. */
const DARKEST_FLOOR = 2;

/** Luminancja wzgledna, wzor WCAG. */
function luminance(colour: string): number {
  const hex = colour.replace('#', '');
  const full = hex.length === 3 ? [...hex].map((one) => one + one).join('') : hex;
  const [r = 0, g = 0, b = 0] = [0, 2, 4].map((at) => {
    const channel = parseInt(full.slice(at, at + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

const contrast = (one: number, other: number): number =>
  (Math.max(one, other) + 0.05) / (Math.min(one, other) + 0.05);

describe('pasmo kontrastu', () => {
  /* Kontrola wzoru na dwoch koncach skali. Barwy sa tu SKLADANE, a nie wpisane: `quick-tokens`
   * odrzuca w `src/**` kazdy literal barwy i ma racje wobec komponentow — ten plik zadnej barwy
   * nie maluje, tylko sprawdza swoj wlasny wzor. */
  it('control: the same formula gives white one and black zero', () => {
    expect(luminance('#' + 'f'.repeat(6))).toBeCloseTo(1, 5);
    expect(luminance('#' + '0'.repeat(6))).toBeCloseTo(0, 5);
    expect(contrast(1, 0)).toBeCloseTo(21, 1);
  });

  it('keeps every drawing inside the band', () => {
    for (const name of DRAWINGS) {
      const src = svg(name);
      const subject = subjectColours(src);
      const ground = groundColours(src);
      expect(subject.length, 'no subject colour was read out of ' + name).toBeGreaterThan(0);
      expect(ground.length, 'no ground colour was read out of ' + name).toBeGreaterThan(0);

      /* Temat swoja NAJJASNIEJSZA barwa, tlo NAJJASNIEJSZYM przystankiem: gradient tla jest
       * wysrodkowany dokladnie tam, gdzie stoi temat, wiec to pod nim lezy ta barwa. */
      const lightestSubject = Math.max(...subject.map(luminance));
      const lightestGround = Math.max(...ground.map(luminance));
      const ratio = contrast(lightestSubject, lightestGround);
      expect(
        ratio,
        name +
          ' puts its subject at ' +
          ratio.toFixed(1) +
          ' : 1 against its ground, which is under the floor. Below three the mark disappears at ' +
          '16 px — that is the WCAG threshold for graphics that are not text.',
      ).toBeGreaterThanOrEqual(FLOOR);
      expect(
        ratio,
        name +
          ' puts its subject at ' +
          ratio.toFixed(1) +
          ' : 1 against its ground. Above nine, a subject that does not fill the tile detaches ' +
          'from it and reads as specks of light rather than one shape — which is what a person ' +
          'looking at the Dock reported.',
      ).toBeLessThanOrEqual(CEILING);

      const darkest = Math.min(...subject.map(luminance));
      const worst = contrast(darkest, lightestGround);
      expect(
        worst,
        name +
          ' lets the darkest part of its subject sit at ' +
          worst.toFixed(1) +
          ' : 1 against the brightest part of its ground. The lightest colour can hold the whole ' +
          'band while the other half of the subject sinks into the tile.',
      ).toBeGreaterThanOrEqual(DARKEST_FLOOR);
    }
  });
});
