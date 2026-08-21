import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-1 dla T-48: pasmo promieni i prawdziwe nazwy docieraja do pieciu sekcji listowych.
 *
 * `rounded-sq` i `bg-*-wash` to ALIASY, ktore T-45 utrzymal przy zyciu wylacznie po to, zeby
 * migracja byla addytywna: `--radius-sq: var(--radius-sm)`, `--color-attend-wash:
 * var(--color-attend-soft)`. Zadanie T-50 je kasuje, a aliasu wolanego z szescdziesieciu osmiu
 * miejsc skasowac sie nie da — powierzchnie zostaja wtedy bez ani jednej reguly CSS, czyli
 * z awaria, ktora nie rzuca wyjatku i widac ja tylko okiem.
 *
 * CZYTANE ZE ZRODEL, nie z wyrenderowanego ekranu, i to jest tu wlasciwe: pytanie brzmi „czy
 * w kodzie tych sekcji zostal choc jeden alias", a nie „co widzi czlowiek". Alias na sciezce,
 * ktora renderuje sie raz na tydzien, jest tym samym dlugiem co alias na widoku glownym.
 *
 * SLABA WERSJA: asercja, ze `rounded-md` gdzies jest. Przechodzi z szescdziesiecioma nazwami
 * zastepczymi obok — czyli na dzisiejszym stanie plus jedna linia.
 *
 * CZEGO TO KRYTERIUM NIE PILNUJE, I JEST TO PRZYJETE SWIADOMIE: nie widzi powierzchni, ktora
 * promien STRACILA. Nazwy klas nie odpowiadaja na to pytanie, bo promien legalnie WYPROWADZA sie
 * z klasy narzedziowej do klasy domu — `more-settings.tsx` nie ma dzis ani jednego `rounded-*`
 * i jest to poprawne, bo jego pola biora `.field`. Pilnuja tego dwie inne rzeczy: kryterium AC-4,
 * ktore czyta definicje `.field` w arkuszu, oraz makieta, ktora jest wyrocznia wygladu i ma
 * promien wpisany w kazda regule powierzchni.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SECTIONS = ['agents', 'skills', 'memory', 'workflows', 'triggers'] as const;
const BAND = ['sm', 'md', 'lg', 'pill'];

/* Zrodlo bez komentarzy blokowych, i to nie jest ostroznosc na zapas: naglowek
 * `workflows/step-panel/panel.tsx` CYTUJE `<textarea id="step-instructions">` w opisie awarii,
 * ktora naprawia. Skaner czytajacy komentarze widzi tam kontrolke bez ani jednej klasy i melduje
 * defekt w kodzie, ktory jest poprawny — a kiedy indziej odwrotnie: regula wpisana do komentarza
 * przechodzi jako regula prawdziwa. `checks/quick-tokens.sh` ma na to `strip_comments` z tego
 * samego powodu. */
const withoutRemarks = (source: string): string => source.replace(/\/\*[\s\S]*?\*\//g, ' ');

/** Wszystkie pliki zrodlowe sekcji, bez testow — test wolno pisac o nazwie zastepczej. */
function sources(): readonly (readonly [string, string])[] {
  const out: [string, string][] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name)) {
        out.push([path.slice(ROOT.length + 1), withoutRemarks(readFileSync(path, 'utf8'))]);
      }
    }
  };
  for (const one of SECTIONS) walk(resolve(ROOT, 'src', 'sections', one));
  return out;
}

describe('pasmo promieni w sekcjach', () => {
  const files = sources();
  const radii = files.flatMap(([path, text]) =>
    [...text.matchAll(/\brounded-([a-z0-9[\]./%-]+)/g)].map((hit) => [path, hit[1] ?? ''] as const),
  );

  it('read enough to judge', () => {
    expect(files.length, 'fewer files were read than these five sections hold').toBeGreaterThan(11);
    expect(
      radii.length,
      'almost no corner names were read, so every assertion below would pass on an empty list',
    ).toBeGreaterThan(19);
  });

  it('keeps not one of the two names that stand in for the real ones', () => {
    const left = radii.filter(([, name]) => name === 'sq' || name === 'dot');
    expect(
      left,
      'these places still name a stand-in corner: ' +
        JSON.stringify(left) +
        '. It resolves to the real one today and disappears in the next task; every place that ' +
        'names it is then left with no rule at all, which is a failure nothing throws on.',
    ).toEqual([]);
  });

  it('keeps not one stand-in colour either', () => {
    const left = files.flatMap(([path, text]) =>
      [...text.matchAll(/\b(?:bg|border|text)-[a-z]+-wash\b/g)].map(
        (hit) => [path, hit[0]] as const,
      ),
    );
    expect(
      left,
      'these places still name a stand-in colour: ' +
        JSON.stringify(left) +
        '. Same story as the corner: it points at the real one and dies in the next task.',
    ).toEqual([]);
  });

  it('names only corners the house has', () => {
    const outside = radii.filter(([, name]) => !BAND.includes(name));
    expect(
      outside,
      'these corners are outside the four the house owns (' +
        BAND.join(', ') +
        '): ' +
        JSON.stringify(outside) +
        '. A fifth corner is a fifth decision, and a bracketed value is a decision written where ' +
        'nobody can find it again.',
    ).toEqual([]);
  });

  it('really uses the two that carry cards and chips', () => {
    for (const want of ['md', 'pill']) {
      expect(
        radii.some(([, name]) => name === want),
        'not one place in these five sections asks for the ' +
          want +
          ' corner, and they hold both cards and chips. Everything landing on the smallest ' +
          'corner is the old square language under new names.',
      ).toBe(true);
    }
  });

  /* PER SEKCJA, nie w sumie. Zmierzone dwiema kontrolami negatywnymi 2026-08-19:
   *
   *   1. „gdzies w tych pieciu sekcjach jest promien sredni" przechodzi takze wtedy, gdy CZTERY
   *      z nich wrocily na kwadrat — jedno wystapienie w piatej wystarcza calej piatce.
   *   2. „ta sekcja uzywa wiecej niz jednego promienia" przechodzi po zwinieciu kart do promienia
   *      kontrolki, bo chip zostawia w zbiorze druga nazwe.
   *
   * Dlatego warunek jest postawiony na POJEMNIKU: kazda z tych pieciu sekcji jest lista, a lista
   * ma kafelek, karte albo panel — i to jest struktura, ktorej nie da sie stracic, zostajac lista.
   * Chip per sekcja nie jest wymagany: sekcja, ktora naprawde nie ma nic w stanie, nie ma byc
   * zmuszana do dorobienia sobie chipa, zeby kryterium zzielenialo. */
  it('gives EVERY one of the five a container corner, not just the five together', () => {
    for (const section of SECTIONS) {
      const mine = radii
        .filter(([path]) => path.startsWith('src/sections/' + section + '/'))
        .map(([, name]) => name);
      expect(mine.length, 'no corner at all was read out of ' + section).toBeGreaterThan(0);
      expect(
        mine.some((name) => name === 'md' || name === 'lg'),
        'section ' +
          section +
          ' gives everything a control corner: ' +
          JSON.stringify([...new Set(mine)]) +
          '. It is a list, so it holds a tile, a card or a panel — and a container that takes the ' +
          'corner of a button is the old square language under a new name.',
      ).toBe(true);
    }
  });
});
