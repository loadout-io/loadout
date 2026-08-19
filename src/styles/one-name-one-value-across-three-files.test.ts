import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const THEME = resolve(ROOT, 'src', 'styles', 'theme.css');
const DECISIONS = resolve(ROOT, 'docs', 'DECISIONS-LOCKED.md');
const DESIGN = resolve(ROOT, 'docs', 'design', 'DESIGN.md');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/* AC-3 dla T-50: jedna nazwa, jedna wartosc, w trzech plikach naraz.
 *
 * `checks/quick-tokens.sh` spina `DESIGN.md` z `theme.css` przy kazdym biegu bramki, ale NIE czyta
 * `DECISIONS-LOCKED.md` — a ten plik stoi nad oboma. Rozjazd miedzy nim a arkuszem jest wiec
 * niewidzialny dla bramki i widzialny wylacznie dla czlowieka, ktory kiedys przeczyta plik decyzji
 * i uwierzy.
 *
 * SLABA WERSJA: porownanie samych akcentow. Rozjazd w jednej z pozostalych nazw przechodzi, a to
 * jest dokladnie ten defekt, ktory zrobil z Intera cala epoke cichej awarii.
 */

/** Pary „nazwa -> wartosc" wypisane w tekscie: `--x` ... `#rrggbb` w tym samym wierszu. */
function named(source: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const line of source.split('\n')) {
    const name = /(--[a-z][\w-]*)/.exec(line)?.[1];
    const value = /(#[0-9a-fA-F]{3,8})\b/.exec(line)?.[1];
    if (name !== undefined && value !== undefined) out.set(name, value.toLowerCase());
  }
  return out;
}

/** Wartosci z arkusza domu, po nazwie — razem z prefiksem `--color-`, ktorego proza nie pisze. */
function sheetValues(): Map<string, string> {
  const out = new Map<string, string>();
  for (const hit of text(THEME).matchAll(/(--[a-z][\w-]*)\s*:\s*([^;]+);/g)) {
    out.set(hit[1] ?? '', (hit[2] ?? '').trim().toLowerCase());
  }
  return out;
}

/** Ta sama nazwa w arkuszu: proza pisze `--line`, arkusz `--color-line`. */
function inSheet(name: string, sheet: Map<string, string>): string | undefined {
  return sheet.get(name) ?? sheet.get(name.replace(/^--/, '--color-'));
}

describe('trzy pliki, jedna wartosc', () => {
  const decisions = named(text(DECISIONS));
  const design = named(text(DESIGN));
  const sheet = sheetValues();

  it('read all three files', () => {
    expect(sheet.size, 'no value was read out of the house sheet').toBeGreaterThan(30);
    expect(
      design.size,
      'no name-with-value pair was read out of the design document',
    ).toBeGreaterThan(10);
    expect(text(DECISIONS).length, 'the locked decisions could not be read').toBeGreaterThan(1000);
  });

  it('agrees with the house sheet on every name the decision spells out', () => {
    const wrong: string[] = [];
    let compared = 0;
    for (const [name, value] of decisions) {
      const mine = inSheet(name, sheet);
      if (mine === undefined) {
        wrong.push(name + ' is named in the locked decision and nowhere in the sheet');
        continue;
      }
      compared += 1;
      if (!mine.includes(value))
        wrong.push(name + ': decision says ' + value + ', sheet says ' + mine);
    }
    expect(
      wrong,
      'the locked decision and the house sheet disagree: ' +
        JSON.stringify(wrong) +
        '. That file is read as the truth about what this app looks like, and nothing in the gate ' +
        'was comparing the two.',
    ).toEqual([]);
    expect(
      compared + decisions.size,
      'no pair at all was compared, so this assertion passed without judging anything',
    ).toBeGreaterThan(0);
  });

  it('agrees with the design document too, on the names both spell out', () => {
    const wrong: string[] = [];
    let compared = 0;
    for (const [name, value] of decisions) {
      const mine = design.get(name);
      if (mine === undefined) continue;
      compared += 1;
      if (mine !== value) wrong.push(name + ': decision says ' + value + ', design says ' + mine);
    }
    expect(
      wrong,
      'the locked decision and the design document disagree: ' + JSON.stringify(wrong),
    ).toEqual([]);
    expect(
      compared,
      'not one name is spelled out with its value in BOTH the decision and the design document, ' +
        'so this comparison judged nothing at all',
    ).toBeGreaterThan(0);
  });
});
