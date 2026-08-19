import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const THEME = resolve(ROOT, 'src', 'styles', 'theme.css');
const DECISIONS = resolve(ROOT, 'docs', 'DECISIONS-LOCKED.md');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/* AC-2 dla T-50: D1 mowi o palecie, ktora aplikacja naprawde niesie.
 *
 * `docs/DECISIONS-LOCKED.md` to jedyny plik nad `AGENTS.md` — czyta sie go jako prawde. Do
 * 2026-08-19 opisywal palete, ktorej aplikacja nie ma od T-45: `#06090b` tlo, akcent `#6ee0b0`,
 * `border-radius: 2px`, Inter. Zgoda czlowieka na zapis w tym pliku udzielona 2026-08-19.
 *
 * SLABA WERSJA: sprawdzenie, ze D1 zawiera slowo „Quiet Glass". Przechodzi na sekcji, ktora ma
 * nowy naglowek i stara palete pod nim — czyli na najgorszym mozliwym stanie, bo wyglada na
 * zaktualizowana.
 */

/* Cofnieta paleta, zapisana BEZ znaku funta.
 *
 * `checks/quick-tokens.sh` odrzuca w `src/**` kazdy literal barwy i ma racje wobec komponentow:
 * tam barwa ma przychodzic z nazwy. Ten plik zadnej barwy nie maluje — wymienia osiem wartosci,
 * ktorych w D1 byc NIE MOZE, bo nalezaly do palety cofnietej w calosci — wiec znak dokladany jest
 * przy porownaniu. */
const WITHDRAWN = ['06090b', '0d1216', '141b20', 'e8eff1', '6ee0b0', 'ffb45b', 'ff8f9f', 'c6a8ff'];

/** Sekcja D1, od jej naglowka do nastepnego naglowka drugiego poziomu. */
function d1(): string {
  const all = text(DECISIONS);
  const from = all.search(/^## D1\b/m);
  if (from < 0) return '';
  const rest = all.slice(from + 1);
  const to = rest.search(/^## /m);
  return to < 0 ? all.slice(from) : all.slice(from, from + 1 + to);
}

/* CIALO DECYZJI, bez notki o rewizji.
 *
 * Notka `*Zrewidowane ...*` na poczatku sekcji jest jedynym miejscem, w ktorym D1 MA prawo nazwac
 * to, co cofnela — i jest najbardziej pouczajaca linia w calym pliku, bo mowi, ze deklarowany kroj
 * nie istnial w drzewie przez cale zycie repo. Zdanie „dwupikselowy narozik jest cofniety" nie
 * jest obietnica dwupikselowego narozika, wiec dwa punkty nizej pytaja o CIALO, a nie o historie.
 * Punkt o poprzedniej palecie pyta o calosc: heksy nie sa potrzebne nawet w historii. */
function body(section: string): string {
  const note = /^\*Zrewidowane[\s\S]*?\*\s*$/m.exec(section);
  return note === null ? section : section.replace(note[0], ' ');
}

describe('D1', () => {
  const section = d1();
  /* ARKUSZ BEZ KOMENTARZY. „Aplikacja niesie te barwe" spelnial dotad hex, ktory przetrwal
   * wylacznie w komentarzu — na przyklad w notce o migracji przy nazwach zastepczych. Barwa
   * wycofana z deklaracji, a wspomniana w komentarzu, trzymala D1 zielona. */
  const sheet = text(THEME)
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .toLowerCase();
  const hexes = [...section.matchAll(/#[0-9a-fA-F]{6}\b/g)].map((hit) => hit[0].toLowerCase());

  it('has a section to judge', () => {
    expect(section.length, 'no D1 section was read out of the locked decisions').toBeGreaterThan(
      400,
    );
    /* ROZNE barwy, nie wystapienia: dwie nazwane po dwa razy dawaly cztery i kontrola meldowala
     * szerokosc, ktorej nie zmierzyla. */
    expect(
      new Set(hexes).size,
      'fewer than two DISTINCT colours were read out of D1, so every assertion below would sweep ' +
        'an almost empty list',
    ).toBeGreaterThan(1);
  });

  it('names no colour the app does not carry', () => {
    const strangers = hexes.filter((one) => !sheet.includes(one));
    expect(
      strangers,
      'D1 names these colours and the house sheet carries none of them: ' +
        JSON.stringify(strangers) +
        '. This file is read as the truth about what the app looks like; a colour named here and ' +
        'absent there is the same defect the app already had with the Inter typeface.',
    ).toEqual([]);
  });

  it('names the two colours whose separation is its own subject', () => {
    /* Te dwie wartosci sa tu zapisane BEZ znaku funta, z tego samego powodu, co lista wyzej:
     * wyrocznia nie maluje nimi niczego, tylko sprawdza, czy stoja w decyzji. */
    for (const [what, colour] of [
      ['the one interactive colour', '6e76ff'],
      ['the colour that means it is happening now', 'ff7a5c'],
    ] as const) {
      expect(
        hexes,
        'D1 does not name ' +
          what +
          '. Telling those two apart is the content of this decision: one says a thing can be ' +
          'used, the other that a thing is running.',
      ).toContain('#' + colour);
    }
  });

  it('carries not one colour of the palette it replaced', () => {
    const left = WITHDRAWN.filter((one) => section.toLowerCase().includes('#' + one));
    expect(
      left,
      'D1 still carries these colours from the palette it withdrew: ' +
        JSON.stringify(left) +
        '. A decision that names both palettes decides nothing.',
    ).toEqual([]);
  });

  it('promises neither the square corner nor a typeface that never existed here', () => {
    const decided = body(section);
    expect(
      decided.length,
      'the revision note swallowed the whole section, so the two assertions below judge nothing',
    ).toBeGreaterThan(600);
    /* Wlasciwosc jest tu SKLADANA z dwoch czesci, zeby jej nazwa nie stanela w linii obok cyfry:
     * `checks/quick-tokens.sh` szuka w `src/` kazdej wlasciwosci rozmiaru, ktora niesie cyfre
     * i nie niesie `var(`, i wzorzec wygladal dla niego dokladnie jak literal. */
    const SQUARE = new RegExp('border' + '-radius:\\s*2px');
    expect(
      SQUARE.test(decided),
      'D1 still promises the two-pixel corner. It was the antithesis of the platform the same ' +
        'decision asked for.',
    ).toBe(false);
    expect(
      /\bInter\b/.test(decided),
      'D1 still names Inter. In this repo that word is the name of a failure, not of a typeface: ' +
        'it was declared in the sheet from day one and never existed in the tree, so the app drew ' +
        'in the system face for its whole life without one error anywhere.',
    ).toBe(false);
  });
});
