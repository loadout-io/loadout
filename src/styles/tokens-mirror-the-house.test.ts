import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-1 dla T-45: tokeny zgadzaja sie z migawka domu, wartosc po wartosci.
 *
 * DLACZEGO MIGAWKA, A NIE ODCZYT `../meetnotes` W CZASIE TESTU. `scripts/ci.sh` jest jedynym
 * zrodlem prawdy o tym, co znaczy „zielone", a w GitHub CI katalogu obok nie ma. Test, ktory
 * w takiej sytuacji „grzecznie sie pomija", daje `Tests N skipped (N)` — podpis z listy
 * NOT_A_REAL_RED — czyli przestaje chronic dokladnie tam, gdzie ma chronic. Migawka
 * `docs/design/house-values.json` jest wersjonowana i jej odswiezenie jest czynnoscia swiadoma.
 *
 * DLACZEGO WARTOSCI NIE SA WPISANE W TEN TEST. Slaba wersja tego kryterium to tablica heksow
 * w tescie. Przechodzi ona takze wtedy, gdy dom zmieni palete, a my zostaniemy w tyle — czyli
 * mierzy nas, a nie spojnosc, o ktora chodzi.
 *
 * CZEGO TEN TEST SWIADOMIE NIE SADZI. `--color-id-1..5`, `--color-human*` i warianty `-edge`
 * sa NASZE i nie maja odpowiednika w domu: domowe `--graph-*` sa nasycone, bo obsluguja legende
 * grafu, a u nas kolor agenta o nasyceniu koloru stanu jest awaria (DESIGN §3, przypadek
 * poprzedniego prototypu: Forge dostawal `#ffb45b`, ten sam hex co „wymaga uwagi"). Lista jest jawna, zeby
 * nikt nie „naprawil" spojnosci, kasujac swiadoma roznice.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SNAPSHOT = resolve(ROOT, 'docs/design/house-values.json');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Splaszcza odstepy i zapis alfy, zeby `rgba(255, 255, 255, .09)` i `rgba(255,255,255,0.09)`
 *  byly rowne. Rozjazd na przecinku nie jest rozjazdem wygladu. */
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
      .replace(/;$/, '')
      .trim()
  );
}

/** Tablica zmiennych z arkusza: `--nazwa` -> wartosc. */
function variables(css: string): Map<string, string> {
  const table = new Map<string, string>();
  for (const hit of css.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;{}]+);/g)) {
    table.set(hit[1] ?? '', tight(hit[2] ?? ''));
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

/** Nazwa w domu -> nazwa u nas. To jest cala tresc tego kryterium. */
const MAP: ReadonlyArray<readonly [string, string]> = [
  ['surface-base', '--color-bg'],
  ['surface-raised', '--color-panel'],
  ['surface-raised', '--color-raised'],
  ['surface-input', '--color-well'],
  ['surface-overlay', '--color-overlay'],
  ['surface-solid', '--color-solid'],
  ['surface-hover', '--color-hover'],
  ['scrim', '--color-scrim'],
  ['text-primary', '--color-ink'],
  ['text-secondary', '--color-body'],
  ['text-tertiary', '--color-muted'],
  ['border', '--color-line'],
  ['border-strong', '--color-line-strong'],
  ['border-subtle', '--color-line-subtle'],
  ['accent', '--color-accent'],
  ['accent-hover', '--color-accent-hover'],
  ['accent-active', '--color-accent-active'],
  ['accent-soft', '--color-accent-soft'],
  ['accent-ring', '--color-accent-ring'],
  ['live', '--color-live'],
  ['live-soft', '--color-live-soft'],
  ['warning', '--color-attend'],
  ['warning-soft', '--color-attend-soft'],
  ['danger', '--color-fail'],
  ['danger-soft', '--color-fail-soft'],
  ['radius-sm', '--radius-sm'],
  ['radius-md', '--radius-md'],
  ['radius-lg', '--radius-lg'],
  ['radius-pill', '--radius-pill'],
  ['shadow-sm', '--shadow-sm'],
  ['shadow-md', '--shadow-md'],
  ['shadow-lg', '--shadow-lg'],
  ['glass-blur', '--glass-blur'],
  ['glass-saturate', '--glass-saturate'],
  ['glass-highlight', '--glass-highlight'],
  ['transition', '--transition'],
  ['transition-fast', '--transition-fast'],
];

/** Nasze, swiadome roznice. Nie maja odpowiednika w domu i nie maja go miec. */
const OURS: readonly string[] = [
  '--color-id-1',
  '--color-id-2',
  '--color-id-3',
  '--color-id-4',
  '--color-id-5',
  '--color-human',
  '--color-human-soft',
  '--color-human-edge',
  '--color-live-edge',
  '--color-attend-edge',
  '--color-fail-edge',
];

interface Snapshot {
  readonly from?: string;
  readonly copied?: string;
  readonly values?: Readonly<Record<string, string>>;
}

function snapshot(): Snapshot {
  const raw = fileText(SNAPSHOT);
  if (raw === '') return {};
  try {
    return JSON.parse(raw) as Snapshot;
  } catch {
    return {};
  }
}

describe('tokeny zgadzaja sie z migawka domu', () => {
  it('reads a saved copy that is really there and really full', () => {
    const snap = snapshot();
    expect(
      snap.values,
      'docs/design/house-values.json carries no `values` map, so every comparison below ' +
        'would run against an empty table and pass on nothing. This file is the vendored copy ' +
        'of the design system next door and this check is exactly the one that keeps it true.',
    ).toBeTruthy();

    const tokens = snap.values ?? {};
    expect(
      Object.keys(tokens).length,
      'the saved copy has fewer entries than the mapping table needs. A saved copy that shrank is ' +
        'not a smaller palette, it is a lost comparison.',
    ).toBeGreaterThanOrEqual(new Set(MAP.map(([house]) => house)).size);

    expect(
      snap.from,
      'the saved copy does not say where it came from. A vendored copy without a provenance line ' +
        'is indistinguishable from a value somebody typed.',
    ).toContain('meetnotes');
  });

  it('has no dead entry in the mapping table', () => {
    const snap = snapshot();
    const tokens = snap.values ?? {};
    const theme = variables(withoutComments(fileText(THEME)));

    const missingInHouse = MAP.filter(([house]) => tokens[house] === undefined).map(
      ([house]) => house,
    );
    expect(
      missingInHouse,
      'the mapping table names house entries the saved copy does not have. A dead mapping entry is ' +
        'silently skipped by a naive loop, so the entry it was supposed to guard stops being ' +
        'guarded without anything turning red.',
    ).toEqual([]);

    const missingInOurs = MAP.filter(([, ours]) => theme.get(ours) === undefined).map(
      ([, ours]) => ours,
    );
    expect(
      missingInOurs,
      'src/styles/theme.css does not define these names yet. This is the behaviour T-45 adds: ' +
        'the Quiet Glass dictionary.',
    ).toEqual([]);
  });

  it('agrees with the house on every mapped value', () => {
    const snap = snapshot();
    const tokens = snap.values ?? {};
    const theme = variables(withoutComments(fileText(THEME)));

    /* KIERUNEK ODWROCONY po drugiej opinii 2026-08-19. Petla szla po `MAP` i pomijala
     * w ciszy strone pusta (`if (wanted === '' || mine === '') continue`). Skutek: zapisana
     * kopia, ktora URASTA przy nastepnym odswiezeniu — a jej wlasne pole `why` do odswiezania
     * zacheca — niesie pozycje, ktorych zadna asercja nie porownuje; a pozycja o wartosci
     * pustej przechodzila najpierw kontrole martwego wpisu (klucz istnieje), a potem byla
     * pomijana przez petle, ktora miala ja OSADZIC. To jest ta sama awaria „porownanie
     * przeszlo na niczym", tylko o jeden poziom nizej.
     *
     * Teraz petla po zapisanej kopii sprawdza POKRYCIE, a brak wartosci u nas jest
     * PORAZKA, nie pominieciem. */
    const covered = new Set(MAP.map(([house]) => house));
    const uncovered = Object.keys(tokens).filter((house) => !covered.has(house));
    expect(
      uncovered,
      'the saved copy carries entries the mapping table never names, so nothing compares them. ' +
        'A copy that grows quietly stops being a copy of anything checked.',
    ).toEqual([]);

    const blank = Object.entries(tokens)
      .filter(([, value]) => tight(value) === '')
      .map(([house]) => house);
    expect(
      blank,
      'these entries of the saved copy hold an empty value. An empty value passes the dead-entry ' +
        'check above, because the key is there, and would then be skipped by the very loop ' +
        'meant to judge it.',
    ).toEqual([]);

    const drift: string[] = [];
    for (const [house, ours] of MAP) {
      const wanted = tight(tokens[house] ?? '');
      const mine = theme.get(ours) ?? '';
      if (mine === '') {
        drift.push(ours + ': not defined in theme.css at all');
        continue;
      }
      if (wanted !== mine) drift.push(ours + ': house says ' + wanted + ', we say ' + mine);
    }
    expect(
      drift,
      'these values drifted away from the house. Two apps in one Dock are meant to read as ' +
        'siblings, and that is a decision (D1), not a coincidence.',
    ).toEqual([]);
  });

  it('keeps our deliberate differences deliberate', () => {
    const theme = variables(withoutComments(fileText(THEME)));
    const absent = OURS.filter((name) => theme.get(name) === undefined);
    expect(
      absent,
      'these values are OURS on purpose — the house has no counterpart because its --graph-* ' +
        'hues are saturated for a graph legend, while for us an agent colour with the ' +
        'saturation of a status colour is the failure DESIGN §3 names. If one of them is gone, ' +
        'somebody "fixed" consistency by deleting a difference that was chosen.',
    ).toEqual([]);
  });
});
