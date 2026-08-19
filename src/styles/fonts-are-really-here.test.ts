import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-2 dla T-45: kroje naprawde sa, a deklaracja nie wystarcza.
 *
 * DLACZEGO TO KRYTERIUM ISTNIEJE. `src/styles/theme.css` deklarowal Intera od pierwszego dnia
 * repo, a `find src public -name '*.woff*'` dawal ZERO: katalogu `public/` nie bylo, `@font-face`
 * nie bylo ani jednego. Aplikacja przez caly ten czas rysowala sie krojem systemowym — po cichu,
 * bez ani jednego bledu w konsoli. Komentarz w tamtym pliku sam to przyznawal i zglaszal jako
 * dlug. To jest dokladnie klasa wady, ktora `docs/patterns/03-exit-code-is-not-evidence.md`
 * opisuje: deklaracja wygladajaca na dzialajaca.
 *
 * SLABA WERSJA: `expect(css).toContain('Hanken Grotesk')`. Przechodzi na tym samym defekcie,
 * ktory to kryterium zamyka. Dlatego lancuch ma trzy ogniwa i kazde jest osobna asercja.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

interface FaceRule {
  readonly family: string;
  readonly url: string;
  readonly weight: string;
}

/** Wszystkie bloki `@font-face` z arkusza, kazdy jako trojka (rodzina, plik, waga). */
function faces(css: string): readonly FaceRule[] {
  const out: FaceRule[] = [];
  for (const hit of css.matchAll(/@font-face\s*\{([^}]*)\}/g)) {
    const body = hit[1] ?? '';
    const family = /font-family\s*:\s*["']?([^;"']+)["']?\s*;/.exec(body)?.[1]?.trim() ?? '';
    const url = /src\s*:[^;]*url\(\s*["']?([^"')]+)["']?\s*\)/.exec(body)?.[1]?.trim() ?? '';
    const weight = /font-weight\s*:\s*([^;]+);/.exec(body)?.[1]?.trim() ?? '';
    out.push({ family, url, weight });
  }
  return out;
}

/** Pierwszy czlon listy krojow tokenu, bez cudzyslowow. */
function firstFamily(css: string, token: string): string {
  const value = new RegExp('--' + token + '\\s*:\\s*([^;]+);').exec(css)?.[1] ?? '';
  const first = value.split(',')[0] ?? '';
  return first.replace(/["']/g, '').trim();
}

/** Wszystkie rodziny cytowane w tokenach `--font-*`. Cudzyslow znaczy „nasza, wniesiona". */
function quotedFamilies(css: string): readonly string[] {
  const out: string[] = [];
  for (const hit of css.matchAll(/--font-[a-z-]+\s*:\s*([^;]+);/g)) {
    for (const q of (hit[1] ?? '').matchAll(/["']([^"']+)["']/g)) {
      const name = (q[1] ?? '').trim();
      if (name !== '') out.push(name);
    }
  }
  return out;
}

describe('kroje naprawde sa', () => {
  const css = fileText(THEME);

  it('declares at least one @font-face, because nothing below means anything otherwise', () => {
    expect(
      faces(css).length,
      'src/styles/theme.css declares no @font-face at all, so every point below would loop over ' +
        'an empty list and pass on nothing. Two variable woff2 files are the deliverable: ' +
        'Hanken Grotesk for language, JetBrains Mono for machine values.',
    ).toBeGreaterThan(0);
  });

  it('points every @font-face at a file that exists and is not empty', () => {
    const broken: string[] = [];
    for (const face of faces(css)) {
      const path = resolve(dirname(THEME), face.url);
      if (!existsSync(path)) {
        broken.push(face.family + ' -> ' + face.url + ' (no such file)');
        continue;
      }
      if (statSync(path).size === 0) broken.push(face.family + ' -> ' + face.url + ' (0 bytes)');
    }
    expect(
      broken,
      'a @font-face rule naming a file that is not there gives a 404 on every start and the same ' +
        'fallback face, silently. That is worse than no rule at all, because it looks solved.',
    ).toEqual([]);
  });

  it('makes the bundled family the FIRST member of its family list', () => {
    const ui = firstFamily(css, 'font-ui');
    const mono = firstFamily(css, 'font-mono');
    const declared = faces(css).map((face) => face.family);

    expect(
      declared,
      'the first family the ui list asks for is not one we bundle, so on any machine without ' +
        'it installed the app draws in the fallback — which is exactly what happened with Inter.',
    ).toContain(ui);
    expect(declared).toContain(mono);
    expect(ui, 'the ui list names no family at all').not.toBe('');
    expect(mono, 'the mono list names no family at all').not.toBe('');
  });

  it('has no quoted family without a @font-face, so the rule holds both ways', () => {
    const declared = new Set(faces(css).map((face) => face.family));
    const orphans = quotedFamilies(css).filter((name) => !declared.has(name));
    expect(
      orphans,
      'these families are quoted in a --font-* list and have no @font-face. A quoted family is ' +
        'a promise that the file is in the tree; without the rule it is the Inter defect again.',
    ).toEqual([]);
  });

  it('asks for a weight RANGE, because both files are variable', () => {
    const flat = faces(css).filter((face) => !/\d+\s+\d+/.test(face.weight));
    expect(
      flat,
      'these @font-face rules pin a single weight. Both files are variable (Hanken 100-900, ' +
        'JetBrains 100-800) and a single weight throws away most of a 34 kB payload while ' +
        'making 600 synthesise instead of render.',
    ).toEqual([]);
  });
});
