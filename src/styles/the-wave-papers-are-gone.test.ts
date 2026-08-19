import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const DECISIONS = resolve(ROOT, 'docs', 'DECISIONS-LOCKED.md');
const DESIGN = resolve(ROOT, 'docs', 'design', 'DESIGN.md');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/* AC-4 dla T-50: papiery fali nie istnieja, a nic poza `tasks/` ich nie cytuje.
 *
 * Spec i plan mowia o sobie same, ze sa przejsciowe: „dokument znika, kiedy fala lada". Ich tresc
 * jest rozprowadzona — decyzje do D1, wartosci i komponenty do `DESIGN.md`, kolejnosc do
 * wyladowanych zadan. Zostawiony spec staje sie trzecim zrodlem prawdy o wygladzie i pierwszym,
 * ktore sie rozjedzie.
 *
 * CZTERY PLIKI ZADAN CYTUJA SCIEZKE SPECU I TE CYTATY ZOSTAJA. Plik zadania jest zamrozonym
 * kontraktem wyladowanej galezi; przepisanie go po fakcie falsyfikuje zapis tego, co naprawde bylo
 * sadzone. Skan pyta wiec o cytaty POZA `tasks/`.
 *
 * SLABA WERSJA: `expect(existsSync(spec)).toBe(false)`. Przechodzi po `rm`, zostawiajac w repo
 * cztery odnosniki do pliku, ktorego nie ma — a czesc z nich w dokumencie, ktory zostaje jedynym
 * zrodlem.
 */

const PAPERS = [
  'docs/superpowers/specs/2026-08-19-quiet-glass-design.md',
  'docs/superpowers/plans/2026-08-19-quiet-glass.md',
] as const;

const SKIP = new Set(['node_modules', 'dist', 'target', '.git', 'tasks', 'runs', '.loadout']);

/* DWA PLIKI MAJA PRAWO WYMOWIC TE SCIEZKI.
 *
 * Ten test — bo bez ich nazwania nie da sie o nie zapytac; to ta sama zasada, co przy nazwach
 * zastepczych: plik, ktory czegos ZABRANIA, musi to napisac. Oraz `TASK.md`, czyli kopia
 * zamrozonego kontraktu tego zadania: `tasks/` jest pominiete wyzej dokladnie dlatego, ze plik
 * zadania zapisuje, co bylo prawda w chwili, gdy zadanie powstalo, a przepisanie go po fakcie
 * falsyfikuje ten zapis. Kopia w korzeniu jest tym samym plikiem. */
const MAY_NAME = new Set(['TASK.md', 'src/styles/the-wave-papers-are-gone.test.ts']);

/** Kazdy plik tekstowy repo poza katalogami, ktore sa wynikiem pracy albo zamrozonym zapisem. */
function papersAndCode(): readonly (readonly [string, string])[] {
  const out: [string, string][] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (SKIP.has(entry.name)) continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.(?:md|ts|tsx|rs|sh|json|css|ya?ml)$/.test(entry.name)) {
        out.push([path.slice(ROOT.length + 1), readFileSync(path, 'utf8')]);
      }
    }
  };
  walk(ROOT);
  return out;
}

describe('papiery fali', () => {
  it('are gone from the tree', () => {
    for (const one of PAPERS) {
      expect(
        existsSync(resolve(ROOT, one)),
        one +
          ' is still here. It says of itself that it goes when the wave lands; kept, it becomes ' +
          'a third source of truth about how this app looks, and the first one to drift.',
      ).toBe(false);
    }
  });

  it('are cited by nothing that stays', () => {
    const files = papersAndCode();
    expect(
      files.length,
      'fewer than thirty files were read, so this sweep is over a fragment of the repository',
    ).toBeGreaterThan(29);
    const citing: string[] = [];
    for (const [path, source] of files) {
      if (MAY_NAME.has(path)) continue;
      for (const one of PAPERS) {
        if (source.includes(one)) citing.push(path + ' -> ' + one);
      }
    }
    expect(
      files.filter(([path]) => MAY_NAME.has(path)).length,
      'neither of the two files allowed to name those paths was read, so the exemption above is ' +
        'hiding nothing and the sweep may be reading the wrong tree',
    ).toBe(MAY_NAME.size);
    expect(
      citing,
      'these files that stay point at a document that is gone: ' +
        JSON.stringify(citing) +
        '. A dead link in the one document that is left as the source is worse than the document ' +
        'it points at.',
    ).toEqual([]);
  });

  it('left their content in the files that stay', () => {
    const design = text(DESIGN);
    for (const [what, pattern] of [
      ['the brand', /^## 2\. Marka/m],
      ['colour', /^## 3\. Kolor/m],
      ['type', /^## 4\. Typografia/m],
      ['space and shape', /^## 5\. Przestrzen|^## 5\. Przestrzeń/m],
      ['components', /^## 6\. Komponenty/m],
    ] as const) {
      expect(
        pattern.test(design),
        'the design document has no section about ' +
          what +
          ', so deleting the wave papers deleted content instead of absorbing it',
      ).toBe(true);
    }
    expect(
      /^## D1 — Wyglad: Loadout Quiet Glass|^## D1 — Wygląd: Loadout Quiet Glass/m.test(
        text(DECISIONS),
      ),
      'the locked decision does not carry the heading the wave was supposed to leave behind',
    ).toBe(true);
  });
});
