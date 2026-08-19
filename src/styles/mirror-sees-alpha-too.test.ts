import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-3 dla T-45: lustro DESIGN.md <-> theme.css widzi takze tokeny rgba.
 *
 * LUKA, KTORA TO ZAMYKA. `checks/quick-tokens.sh` porownuje oba pliki wzorcem
 * `#[0-9a-fA-F]{6}`. Ten wzorzec NIE WIDZI `rgba()`. W starej palecie wszystkie 21 tokenow
 * bylo heksami, wiec luka nie miala znaczenia ani przez chwile. W Quiet Glass wiekszosc
 * powierzchni i WSZYSTKIE obrysy to biel-alfa — czyli tamto sprawdzenie przestaje pilnowac
 * ponad polowy palety, meldujac przy tym zielono i wypisujac „N colour tokens agree".
 * Dokladnie ta awaria, przed ktora tamten plik stoi we wlasnym naglowku.
 *
 * DLACZEGO TU, A NIE W `checks/`. `AGENTS.md` §7 wymaga na `checks/` zgody czlowieka, a szersza
 * wyrocznia po stronie testow jest tansza i mniej ryzykowna: nie moze unieruchomic bramki
 * calego repo.
 *
 * NADMIAROWOSC JEST NAZWANA, NIE PRZEMILCZANA. Ten test jest scislym NADZBIOREM polowy 1
 * z `quick-tokens.sh`: sadzi i heksy, i rgba. Dwa egzekutory jednej reguly, nigdy dwie reguly.
 * WARUNEK JEGO ZNIKNIECIA: kiedy czlowiek zgodzi sie rozszerzyc tamten wzorzec o `rgba`,
 * polowa heksowa tego pliku staje sie zbedna i ma zostac usunieta.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const DESIGN = resolve(ROOT, 'docs/design/DESIGN.md');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Znosi rozjazdy zapisu, ktore nie sa rozjazdami wygladu: odstepy i wiodace zero w alfie. */
function tight(value: string): string {
  return (
    value
      .toLowerCase()
      .replace(/\s+/g, '')
      .replace(/([(,])0\./g, '$1.')
      /* Zero na koncu czesci dziesietnej nie jest roznica wygladu, tak samo jak odstep.
       * ZMIERZONE 2026-08-19: prettier normalizuje w `theme.css` `0.10` do `0.1`, a kopia domu
       * niesie `0.10` doslownie. Normalizacja nalezy TUTAJ — poprawianie kopii pod nasz
       * formatter zamienilo by wersjonowany odpis w cos, co juz nie jest odpisem. */
      .replace(/(\.\d*?)0+(?=\D|$)/g, '$1')
      .replace(/\.(?=\D|$)/g, '')
      .trim()
  );
}

/* DESIGN.md podaje tokeny na trzy sposoby: wierszem tabeli, blokiem kodu i wyliczeniem po
 * srodniku. Jeden wzorzec obsluguje wszystkie: nazwa, potem wylacznie backticki / spacja /
 * jedna pionowa kreska, potem wartosc az do granicy. */
const TOKEN = /--([a-z][a-z0-9-]*)`?\s*\|?\s*`?(#[0-9a-fA-F]{6}\b|rgba\([^)]*\))/g;

function fromDesign(): Map<string, string> {
  const table = new Map<string, string>();
  for (const hit of withoutComments(fileText(DESIGN)).matchAll(TOKEN)) {
    table.set(hit[1] ?? '', tight(hit[2] ?? ''));
  }
  return table;
}

function fromTheme(): Map<string, string> {
  const table = new Map<string, string>();
  const re = /--color-([a-z][a-z0-9-]*)\s*:\s*(#[0-9a-fA-F]{6}\b|rgba\([^)]*\))\s*;/g;
  for (const hit of withoutComments(fileText(THEME)).matchAll(re)) {
    table.set(hit[1] ?? '', tight(hit[2] ?? ''));
  }
  return table;
}

const isAlpha = (value: string): boolean => value.startsWith('rgba(');

/* Komentarz NIE jest kodem, a ten test czyta CSS jako tekst.
 *
 * ZMIERZONE 2026-08-19, na tym pliku: komentarz w `theme.css` wyjasniajacy zmiane cytowal
 * regule doslownie — „BYLO: `.text-label { text-transform: uppercase }`" — i parser wzial ten
 * cytat za zywa regule. Test byl czerwony na kodzie, ktory jest POPRAWNY. `checks/quick-tokens.sh`
 * ma na to `strip_comments` i wlasnie dlatego: dokumentacja obok kodu jest w tym repo gesta,
 * wiec parser, ktory jej nie odejmuje, sadzi proze. */
function withoutComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, ' ');
}

describe('lustro widzi takze alfe', () => {
  it('read something out of both sides, or nothing below means anything', () => {
    expect(
      fromDesign().size,
      'no colour value was read out of docs/design/DESIGN.md, so every comparison below would ' +
        'run between two empty tables. Either §3 moved or its shape changed.',
    ).toBeGreaterThan(10);
    expect(
      fromTheme().size,
      'no --color-* value was read out of src/styles/theme.css.',
    ).toBeGreaterThan(10);
  });

  it('compares a NON-ZERO number of alpha values, because zero is not agreement', () => {
    const design = fromDesign();
    const theme = fromTheme();
    const shared = [...design.keys()].filter((name) => theme.has(name));
    const alpha = shared.filter((name) => isAlpha(design.get(name) ?? ''));
    expect(
      alpha.length,
      'zero alpha values were compared. In Quiet Glass most surfaces and every border line are ' +
        'white-alpha, so a mirror that compares none of them is the exact failure this point ' +
        'exists for: green on nothing measured. Compared ' +
        String(shared.length) +
        ' values in total.',
    ).toBeGreaterThan(4);
  });

  it('agrees on every shared value, alpha and hex alike', () => {
    const design = fromDesign();
    const theme = fromTheme();
    const drift: string[] = [];
    for (const [name, wanted] of design) {
      const mine = theme.get(name);
      if (mine === undefined) continue;
      if (mine !== wanted)
        drift.push('--' + name + ': DESIGN says ' + wanted + ', theme says ' + mine);
    }
    expect(
      drift,
      'DESIGN.md is the source and theme.css is its mirror (DESIGN §9). A design document that ' +
        'drifted from the code is worse than none: it still looks like the source and is still ' +
        'quoted in reviews.',
    ).toEqual([]);
  });

  it('fails on a name that lives in only one of the two files', () => {
    const design = fromDesign();
    const theme = fromTheme();
    const onlyDesign = [...design.keys()].filter((name) => !theme.has(name));
    const onlyTheme = [...theme.keys()].filter((name) => !design.has(name));
    expect(
      onlyDesign,
      'these names are documented in DESIGN.md and never defined in theme.css',
    ).toEqual([]);
    expect(onlyTheme, 'these names exist in theme.css and DESIGN.md never documents them').toEqual(
      [],
    );
  });
});
