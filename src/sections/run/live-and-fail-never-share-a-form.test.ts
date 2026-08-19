import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-1 dla T-47: `--live` i `--fail` nie dziela ani jednej FORMY.
 *
 * DLACZEGO TA REGULA ISTNIEJE. Obie barwy roznia sie odcieniem o ~13 stopni (#ff7a5c wobec
 * #ff6b6b), a w naszym strumieniu stoja w sasiednich wierszach — czego system, z ktorego
 * wzielismy wartosci, nigdy nie musi pokazac. Rozstrzyga wiec forma, nie barwa: „teraz" jest
 * podkladem i obrysem wiersza, „zepsute" jest glifem i krawedzia bloku bledu.
 *
 * DLACZEGO STATYCZNIE, NA ZRODLE. To repo nie ma `jsdom` ani `environment` w `vite.config.ts`,
 * wiec vitest biegnie w node, testy renderuja `renderToStaticMarkup`, a `getComputedStyle`
 * nie istnieje. Kryterium oparte na obliczonym stylu nie ruszyloby ANI RAZU i bylo by podpisem
 * z listy NOT_A_REAL_RED, nie czerwienia.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const RUN = resolve(ROOT, 'src', 'sections', 'run');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/** Komentarz nie jest kodem: regula zacytowana w prozie nie jest regula. */
function withoutComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

/** Pliki produkcyjne ekranu Run. */
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

/**
 * Nazwy klas stojace obok danego rdzenia w tym samym napisie klas.
 *
 * Forma elementu to zestaw klas, ktore go opisuja. Jesli ten sam zestaw nosi raz „teraz",
 * a raz „zepsute", to obie barwy sa nieodroznialne dla kazdego, kto nie zna heksow na pamiec.
 */
function formsAround(src: string, stem: string): ReadonlySet<string> {
  const out = new Set<string>();
  for (const hit of src.matchAll(/(?:className|class)\s*=\s*[{]?\s*[`'"]([^`'"]*)[`'"]/g)) {
    const value = hit[1] ?? '';
    if (!new RegExp('\\b[a-z-]*' + stem + '\\b').test(value)) continue;
    for (const word of value.split(/\s+/)) {
      const bare = word.trim();
      if (bare === '') continue;
      if (bare.includes('live') || bare.includes('fail')) continue;
      out.add(bare);
    }
  }
  return out;
}

describe('live i fail nie dziela formy', () => {
  const sources = files().map((f) => [relative(ROOT, f), withoutComments(text(f))] as const);

  it('scanned enough of the run screen to be measuring anything', () => {
    expect(
      sources.length,
      'the walk over src/sections/run found almost no production file, so every point below ' +
        'would loop over an empty list and pass on nothing',
    ).toBeGreaterThan(10);
  });

  it('finds both vocabularies in use, because an empty set is disjoint for free', () => {
    const all = sources.map(([, src]) => src).join('\n');
    expect(
      [...formsAround(all, 'live')].length,
      'no element in the run screen carries the happening-now colour at all, so disjointness ' +
        'below would hold on an empty set — the same pass-on-nothing failure this file exists ' +
        'to prevent',
    ).toBeGreaterThan(0);
    expect(
      [...formsAround(all, 'fail')].length,
      'no element in the run screen carries the broken colour at all',
    ).toBeGreaterThan(0);
  });

  it('keeps the two form vocabularies disjoint', () => {
    const all = sources.map(([, src]) => src).join('\n');
    const nowForms = formsAround(all, 'live');
    const badForms = formsAround(all, 'fail');
    const shared = [...nowForms].filter((form) => badForms.has(form));
    expect(
      shared,
      'these shapes carry BOTH the happening-now colour and the broken colour somewhere in the ' +
        'run screen: ' +
        JSON.stringify(shared) +
        '. The two hues sit 13 degrees apart and stand in neighbouring rows, so the shape is ' +
        'the only thing that tells them apart — and a shape that means both means neither.',
    ).toEqual([]);
  });
});
