import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-1 dla T-47: `--live` i `--fail` nie dziela ani jednej FORMY.
 *
 * DLACZEGO TA REGULA ISTNIEJE. Obie barwy roznia sie odcieniem o ~13 stopni (#ff7a5c wobec
 * #ff6b6b), a w strumieniu stoja w sasiednich wierszach. Rozstrzyga wiec ksztalt: „teraz" jest
 * kropka i podkladem, „zepsute" jest glifem i krawedzia bloku bledu.
 *
 * DLACZEGO STATYCZNIE. To repo nie ma `jsdom` ani `environment` w `vite.config.ts`, wiec vitest
 * biegnie w node i `getComputedStyle` nie istnieje. Kryterium na obliczonym stylu nie ruszylo by
 * ani razu — byloby podpisem z listy NOT_A_REAL_RED, nie czerwienia.
 *
 * DWIE RZECZY POPRAWIONE PO DRUGIEJ OPINII 2026-08-19, obie istotne:
 *
 * 1. SKANER WIDZI MAPY. Poprzednia wersja czytala wylacznie literaly `className="..."`, a ten
 *    kod podaje barwy stanu przez MAPY i zmienne (`BLOCK`, `LABEL` w `strip.tsx`, `tone`
 *    zwracane z `marker()` w `line.tsx`). Prawdziwe nosniki coralu i czerwieni byly wiec dla
 *    niego niewidzialne, a sadzil cztery recznie wybrane napisy.
 * 2. KOLIZJA TO CALA FORMA, nie wspolne slowo. `text-center` i `font-mono` niosa pol aplikacji;
 *    gdyby wspolne slowo tworzylo kolizje, ta regula zabranialaby uzywania siatki i kroju.
 *    Dwa elementy dziela FORME, gdy ich zestawy klas po odjeciu barwy sa IDENTYCZNE — bo wtedy
 *    jedyna roznica miedzy nimi jest odcien.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const RUN = resolve(ROOT, 'src', 'sections', 'run');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

function withoutComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

function files(): readonly string[] {
  const out: string[] = [];
  const walk = (dir: string): void => {
    if (!existsSync(dir)) return;
    for (const name of readdirSync(dir).sort()) {
      const full = join(dir, name);
      if (statSync(full).isDirectory()) {
        if (name !== 'fixtures') walk(full);
        continue;
      }
      if (!/\.(ts|tsx)$/.test(name)) continue;
      if (/\.(test|spec)\.[jt]sx?$/.test(name)) continue;
      out.push(full);
    }
  };
  walk(RUN);
  return out;
}

/* Przedrostki, po ktorych poznajemy, ze napis jest lista klas, a nie zdaniem. */
const CLASSY =
  /(^|\s)(bg|text|border|rounded|shadow|ring|fill|stroke|animate|font|size|grid|flex|gap|opacity|whitespace|truncate|items|justify|content|min|max|shrink|overflow|px|py|pt|pb|pl|pr|mx|my|mt|mb|ml|mr|h|w|inset|absolute|relative|block|inline|hidden|sr)-/;

/** Wszystkie literaly napisow w pliku, ktore wygladaja jak lista klas. */
function classLiterals(src: string): readonly string[] {
  const out: string[] = [];
  for (const hit of src.matchAll(/[`'\x22]([^`'\x22\n]*)[`'\x22]/g)) {
    const value = (hit[1] ?? '').trim();
    if (value === '' || value.includes('${')) continue;
    if (CLASSY.test(' ' + value)) out.push(value);
  }
  return out;
}

/**
 * FORMY z pliku: kazda lista klas, plus kazda lista SKLEJONA z wstawki.
 *
 * Wstawki (`${LABEL[block.state]}`) rozwijamy przez KAZDY literal klasowy z tego samego pliku.
 * Jest to nadmiarowe z premedytacja: moze znalezc kolizje, ktorej dana galaz nigdy nie zestawi,
 * i nie moze przegapic tej, ktora zestawi. Sprawdzenie, ktore myli sie w strone czerwieni,
 * kaze cos uzasadnic; takie, ktore myli sie w strone zieleni, nie kaze niczego.
 */
function forms(src: string): readonly (readonly string[])[] {
  const literals = classLiterals(src);
  const pieces = literals.filter((value) => /^[a-z0-9:\[\]()._/-]+$/.test(value));
  const out: string[][] = [];
  for (const hit of src.matchAll(/[`]([^`]*)[`]/g)) {
    const tpl = hit[1] ?? '';
    if (!tpl.includes('${') || !CLASSY.test(' ' + tpl)) continue;
    const base = tpl
      .replace(/\$\{[^}]*\}/g, ' ')
      .split(/\s+/)
      .filter((one) => one !== '');
    for (const piece of pieces) out.push([...base, ...piece.split(/\s+/)]);
    out.push(base);
  }
  for (const value of literals) {
    if (value.includes('${')) continue;
    out.push(value.split(/\s+/).filter((one) => one !== ''));
  }
  return out;
}

const isLive = (one: string): boolean => /\blive\b|-live\b/.test(one);
const isFail = (one: string): boolean => /\bfail\b|-fail\b/.test(one);

/** Forma to zestaw klas PO ODJECIU barwy, znormalizowany do jednego napisu. */
function shape(classes: readonly string[]): string {
  return [...new Set(classes.filter((one) => !isLive(one) && !isFail(one)))].sort().join(' ');
}

describe('live i fail nie dziela formy', () => {
  const sources = files().map((f) => [relative(ROOT, f), withoutComments(text(f))] as const);

  it('scanned enough of the run screen to be measuring anything', () => {
    expect(
      sources.length,
      'the walk over the run screen found almost no production file, so every point below would ' +
        'loop over an empty list and pass on nothing',
    ).toBeGreaterThan(10);
  });

  it('sees colours handed over by maps, not only by literal attributes', () => {
    /* KONTROLA SAMEGO SKANERA. `strip.tsx` podaje coral przez mape `BLOCK`, a nie w atrybucie,
     * i to wlasnie tego poprzednia wersja nie widziala. */
    const strip = sources.find(([name]) => name.endsWith('strip/strip.tsx'))?.[1] ?? '';
    expect(strip, 'strip.tsx could not be read').not.toBe('');
    expect(
      classLiterals(strip).some((value) => isLive(value)),
      'the scanner does not see the happening-now colour in strip.tsx, where it is handed over ' +
        'by a map rather than written into an attribute. A scanner blind to that judges a few ' +
        'hand-picked strings and reports green about the rest of the screen.',
    ).toBe(true);
  });

  it('finds both vocabularies in use, because an empty set is disjoint for free', () => {
    const all = sources.flatMap(([, src]) => forms(src));
    expect(
      all.filter((one) => one.some(isLive)).length + all.filter((one) => one.some(isFail)).length,
      'neither colour appears on any shape at all, so disjointness below would hold on empty ' +
        'sets — the same pass-on-nothing failure this file exists to prevent',
    ).toBeGreaterThan(0);
  });

  it('keeps the two shape vocabularies disjoint', () => {
    const all = sources.flatMap(([name, src]) => forms(src).map((one) => [name, one] as const));
    const live = new Map<string, string>();
    const bad = new Map<string, string>();
    for (const [name, classes] of all) {
      const key = shape(classes);
      if (key === '') continue;
      if (classes.some(isLive)) live.set(key, name);
      if (classes.some(isFail)) bad.set(key, name);
    }
    expect(live.size, 'no shape carries the happening-now colour').toBeGreaterThan(0);
    expect(bad.size, 'no shape carries the broken colour').toBeGreaterThan(0);

    const shared = [...live.keys()]
      .filter((key) => bad.has(key))
      .map((key) => key + '  [' + String(live.get(key)) + ' / ' + String(bad.get(key)) + ']');
    expect(
      shared,
      'these shapes carry BOTH the happening-now colour and the broken colour: ' +
        JSON.stringify(shared) +
        '. The two hues sit 13 degrees apart and stand in neighbouring rows, so the shape is the ' +
        'only thing that tells them apart — and a shape that means both means neither.',
    ).toEqual([]);
  });
});
