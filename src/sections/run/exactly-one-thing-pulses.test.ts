import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-5 dla T-47: pulsuje dokladnie tyle, ile wolno, i to co ma.
 *
 * `docs/ARCHITECTURE.md` §7 daje sufit „Regiony animujace sie od jednego zdarzenia: 2".
 * Sufit jest CZYTANY z tego pliku po tresci wiersza, nie wpisany: liczba przepisana z palca
 * rozjechalaby sie przy pierwszej zmianie architektury i klamalaby cicho (niezmiennik 18).
 *
 * DESIGN §7 mowi tez, CO ma prawo pulsowac: rzecz, ktora odpowiada na pytanie „co sie dzieje
 * teraz". Kropka gotowosci dostawcy nie jest ani interakcja, ani „teraz" — jest dostepnoscia,
 * i stoi w miejscu.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const ARCHITECTURE = resolve(ROOT, 'docs/ARCHITECTURE.md');
const SRC = resolve(ROOT, 'src');
const THEME = resolve(ROOT, 'src/styles/theme.css');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

function withoutComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

/** Sufit z wiersza tabeli §7, szukany po TRESCI wiersza. */
function ceiling(md: string): number {
  const row = /\|\s*Regiony animuj[^|]*\|([^|]*)\|/.exec(md);
  const digits = /(\d+)/.exec(row?.[1] ?? '');
  return digits === null ? 0 : Number(digits[1]);
}

function productionFiles(): readonly string[] {
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
  walk(SRC);
  return out;
}

/**
 * Napisy klas, ktore niosa animacje, razem z plikiem, z ktorego pochodza.
 *
 * SKANER WIDZI KAZDY LITERAL, nie tylko atrybut — poprawione po drugiej opinii. Ten kod podaje
 * klasy takze przez mapy i zmienne (`BLOCK`, `LABEL`, `tone`), wiec wersja czytajaca wylacznie
 * `className="..."` przepuscilaby trzecia ruszajaca sie rzecz dopisana w tym idiomie.
 *
 * CO TA LICZBA ZNACZY, powiedziane wprost, bo pomiar z nieopisana granica jest gorszy niz jego
 * brak: liczymy MIEJSCA W KODZIE, ktore sie ruszaja, czyli RODZAJE ruszajacej sie rzeczy.
 * Jedno miejsce moze wyrenderowac wiele wystapien — kropka na karcie w tle rysuje sie raz na
 * kazdy folder z zywym biegiem. Sufit z §7 mowi o regionach animujacych sie OD JEDNEGO
 * ZDARZENIA, a trzy kropki na trzech kartach sa jednym rodzajem odpowiedzi na jedno pytanie,
 * nie trzema. Rozroznienie „ile rodzajow" wobec „ile sztuk naraz" jest tu wyborem, nie
 * przeoczeniem; drugiego nie da sie zmierzyc bez uruchomionej aplikacji.
 */
function animated(): ReadonlyArray<readonly [string, string]> {
  const out: Array<readonly [string, string]> = [];
  for (const file of productionFiles()) {
    const src = withoutComments(text(file));
    for (const hit of src.matchAll(/[`'\x22]([^`'\x22\n]*)[`'\x22]/g)) {
      const value = hit[1] ?? '';
      if (/\banimate-[a-z-]+\b/.test(value)) out.push([relative(ROOT, file), value] as const);
    }
  }
  return out;
}

describe('co pulsuje', () => {
  const md = text(ARCHITECTURE);

  it('reads a positive limit out of ARCHITECTURE §7', () => {
    expect(md, 'docs/ARCHITECTURE.md could not be read at all').not.toBe('');
    expect(
      ceiling(md),
      'the limit row could not be parsed out of §7, so the count below would be compared against ' +
        'zero and any amount of movement would pass',
    ).toBeGreaterThan(0);
  });

  it('moves no more places than the limit allows', () => {
    const places = animated();
    expect(
      places.length,
      'these places animate: ' +
        JSON.stringify(places.map(([file]) => file)) +
        ' and §7 allows ' +
        String(ceiling(md)) +
        '. Movement everywhere is movement nowhere: the eye chases it instead of reading.',
    ).toBeLessThanOrEqual(ceiling(md));
  });

  it('moves only what answers "what is happening right now"', () => {
    const places = animated();
    expect(
      places.length,
      'nothing animates at all, so this point measures nothing',
    ).toBeGreaterThan(0);
    const wrong = places
      .filter(([, value]) => !/\b[a-z-]*live\b/.test(value))
      .map(([file, value]) => file + ' -> ' + value);
    expect(
      wrong,
      'these moving things do not carry the happening-now colour, so they move without saying ' +
        'that anything is happening. DESIGN §7 keeps movement for exactly that one fact.',
    ).toEqual([]);
  });

  it('leaves the readiness dot standing still', () => {
    const nav = withoutComments(text(resolve(ROOT, 'src/ui/shell/titlebar.tsx')));
    const footer = /<div[^>]*mt-auto[\s\S]*?<\/div>/.exec(nav)?.[0] ?? '';
    expect(footer, 'no footer was found in the navigation card').not.toBe('');
    expect(
      /animate-/.test(footer),
      'the readiness dot moves. Whether Claude Code can be reached is neither an interaction nor ' +
        'something happening now, and a dot that blinks forever spends one of the two regions §7 ' +
        'allows on a fact that never changes.',
    ).toBe(false);
  });

  it('defines the movement ONCE, in the sheet, not in a component', () => {
    const sheet = withoutComments(text(THEME));
    const defined = [...sheet.matchAll(/@keyframes\s+([a-z-]+)/g)].map((hit) => hit[1] ?? '');
    expect(
      defined.length,
      'the sheet defines no keyframes at all, so `animate-*` names an animation that does not ' +
        'exist and the dot simply sits still',
    ).toBeGreaterThan(0);
    const inComponents = productionFiles().filter((file) =>
      /@keyframes/.test(withoutComments(text(file))),
    );
    expect(
      inComponents.map((file) => relative(ROOT, file)),
      'these components define their own keyframes. A second copy of one fact drifts at the first ' +
        'change of timing, and a drifting animation does not fail — two dots side by side simply ' +
        'blink out of step (invariant 13).',
    ).toEqual([]);
  });
});
