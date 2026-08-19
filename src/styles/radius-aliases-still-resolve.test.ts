import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-4 dla T-45: aliasy promieni zyja i rozwijaja sie do nowego pasma.
 *
 * NIEZMIENNIK 25 ZASTOSOWANY DO CSS. Migracje sa addytywne i idempotentne: nowa nazwa dochodzi,
 * stara zyje jako alias. `--radius-sq` jest dzis wolane przez trzy powierzchnie, ktore migruja
 * dopiero w T-46/T-47/T-48. Nazwa skasowana pod nimi zostawia element BEZ ANI JEDNEJ reguly
 * CSS — awarie, ktora nie rzuca wyjatku i nie pojawia sie w zadnym logu.
 *
 * SEDNO JEST W PUNKCIE O ROZWINIECIU. Alias, ktory wskazuje na wlasna kopie liczby, przy
 * nastepnej zmianie pasma rozjedzie sie po cichu — a wtedy dwie powierzchnie maja dwa rozne
 * promienie „2 px" i nikt tego nie widzi, dopoki nie stana obok siebie.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function variables(css: string): Map<string, string> {
  const table = new Map<string, string>();
  for (const hit of css.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;{}]+);/g)) {
    table.set(hit[1] ?? '', (hit[2] ?? '').replace(/\s+/g, ' ').trim());
  }
  return table;
}

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

/** Rozwija `var(--x)` przez tablice. Cztery przejscia, bo alias moze wskazywac na alias. */
function expand(value: string, table: Map<string, string>): string {
  let out = value;
  for (let round = 0; round < 4; round += 1) {
    const next = out.replace(/var\((--[a-z0-9-]+)\)/g, (whole, name: string) => {
      return table.get(name) ?? whole;
    });
    if (next === out) break;
    out = next;
  }
  return out.trim();
}

const BAND = ['--radius-sm', '--radius-md', '--radius-lg', '--radius-pill'] as const;

describe('aliasy promieni', () => {
  const table = variables(withoutComments(fileText(THEME)));

  it('read the sheet at all', () => {
    expect(
      table.size,
      'no CSS variable was read out of src/styles/theme.css, so every point below would look ' +
        'things up in an empty table and pass on nothing',
    ).toBeGreaterThan(10);
  });

  it('carries the whole new band', () => {
    const missing = BAND.filter((name) => table.get(name) === undefined);
    expect(
      missing,
      'the Quiet Glass radius band is incomplete. The house band is 9 / 13 / 18 / 24 / pill and ' +
        'we take the lower end on purpose.',
    ).toEqual([]);
  });

  it('has no 24px rung, because a tool at this density stops looking like a tool', () => {
    /* WARTOSC, nie pisownia — poprawione po drugiej opinii 2026-08-19. Porownanie napisu
     * z `'24px'` przepuszczalo `24.0px`, `1.5rem` i `calc(12px * 2)`, czyli trzy zapisy
     * dokladnie tego ksztaltu, ktorego ten punkt zabrania. `calc()` odrzucamy WPROST, bo
     * jego wyniku nie da sie tu policzyc, a punkt udajacy, ze potrafi, jest gorszy niz brak. */
    const pixels = (value: string): number | null => {
      const v = value.replace(/\s/g, '');
      const px = /^(\d+(?:\.\d+)?)px$/.exec(v);
      if (px !== null) return Number(px[1]);
      const rem = /^(\d+(?:\.\d+)?)rem$/.exec(v);
      if (rem !== null) return Number(rem[1]) * 16;
      return null;
    };
    const rungs = [...table.entries()].filter(([name]) => name.startsWith('--radius-'));
    const calc = rungs.filter(([, value]) => value.includes('calc(')).map(([name]) => name);
    expect(
      calc,
      'a radius rung is written with calc(), whose result this point cannot evaluate — so the ' +
        'shape it forbids could be reached with nothing going red',
    ).toEqual([]);
    const wrong = rungs
      .filter(([, value]) => pixels(expand(value, table)) === 24)
      .map(([name]) => name);
    expect(
      wrong,
      'a 24px radius rung exists. The house has it; we deliberately do not, because at this ' +
        'information density a 24px corner reads as an iPad app rather than a work tool.',
    ).toEqual([]);
  });

  it('keeps the old names ALIVE, because three surfaces still call them', () => {
    for (const name of ['--radius-sq', '--radius-dot']) {
      expect(
        table.get(name),
        name +
          ' is gone. Three surfaces still call it and migrate only in T-46/T-47/T-48; a name ' +
          'deleted underneath them leaves the element with no CSS rule at all — a failure that ' +
          'throws nothing and logs nothing (invariant 25).',
      ).toBeDefined();
    }
  });

  it('resolves the aliases THROUGH the band, not to their own copy of the number', () => {
    const pairs: ReadonlyArray<readonly [string, string]> = [
      ['--radius-sq', '--radius-sm'],
      ['--radius-dot', '--radius-pill'],
    ];
    for (const [alias, target] of pairs) {
      const raw = table.get(alias) ?? '';
      expect(
        raw,
        alias + ' has no value, so the comparison below would run against an empty string',
      ).not.toBe('');
      /* NAZWA, nie sam ksztalt — poprawione po drugiej opinii 2026-08-19. Warunek „zawiera
       * `var(`" przepuszczal `--radius-sq: var(--radius-old-sq)` z osobnym
       * `--radius-old-sq: 9px`, czyli dokladnie ta druga kopie liczby, ktorej ten punkt
       * zabrania — cicha rozbieznosc przesuwala sie o jedno przekierowanie dalej, a rownosc
       * rozwinietych wartosci spelnia takze zdublowany literal. */
      expect(
        raw.replace(/\s/g, ''),
        alias +
          ' does not name ' +
          target +
          ' directly. An alias holding its own copy of the number — or pointing at a second ' +
          'name that holds one — drifts silently the next time the band moves, and then two ' +
          'surfaces have two different "2px".',
      ).toBe('var(' + target + ')');
      expect(expand(raw, table)).toBe(expand(table.get(target) ?? 'MISSING', table));
    }
  });
});
