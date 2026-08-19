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

/* `.claude` jest tu razem z `node_modules`: narzedzia sesji trzymaja w `.claude/worktrees/<id>`
 * PELNE kopie repo, kazda z wlasnym `TASK.md`, i ten katalog jest w `.gitignore` wlasnie dlatego.
 * Skan, ktory tam zaglada, wydaje werdykt o stanie lokalnym, ktorego nikt czytajacy rownice nie
 * widzi — w obie strony: raz falszywa czerwien, raz cisza. */
const SKIP = new Set([
  'node_modules',
  'dist',
  'target',
  '.git',
  '.claude',
  'tasks',
  'runs',
  '.loadout',
]);

/* DWA PLIKI MAJA PRAWO WYMOWIC TE SCIEZKI.
 *
 * Ten test — bo bez ich nazwania nie da sie o nie zapytac; to ta sama zasada, co przy nazwach
 * zastepczych: plik, ktory czegos ZABRANIA, musi to napisac. Oraz `TASK.md`, czyli kopia
 * zamrozonego kontraktu tego zadania: `tasks/` jest pominiete wyzej dokladnie dlatego, ze plik
 * zadania zapisuje, co bylo prawda w chwili, gdy zadanie powstalo, a przepisanie go po fakcie
 * falsyfikuje ten zapis. Kopia w korzeniu jest tym samym plikiem. */
const SELF = 'src/styles/the-wave-papers-are-gone.test.ts';
/* `TASK.md` jest wyjety, DOPOKI istnieje: jest kopia zamrozonego kontraktu tej galezi i ginie
 * przy ladowaniu (`integrate.sh`). Wyjecie go nie moze byc wiec wymogiem jego obecnosci. */
const MAY_NAME = new Set(['TASK.md', SELF]);

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
    /* KONTROLA PYTA O TEN PLIK, NIE O LICZBE.
     *
     * Pierwsza wersja zadala, zeby przeczytane byly DOKLADNIE dwa wyjete pliki — a `TASK.md` jest
     * artefaktem GALEZI: `integrate.sh` kasuje go przy ladowaniu. Ten warunek zzielenialby tu
     * i zaczerwienil sie na trunku, w `full-test`, razem ze scaleniem i bez zadnej zmiany w kodzie.
     * Sens kontroli jest inny: sprawdzic, ze skan czyta to drzewo, a nie puste. Ten plik zawsze
     * w nim jest — jesli go nie widzi, nie widzi niczego. */
    expect(
      files.map(([path]) => path),
      'the sweep did not even read the file doing the sweeping, so it is looking at the wrong tree',
    ).toContain(SELF);
    expect(
      citing,
      'these files that stay point at a document that is gone: ' +
        JSON.stringify(citing) +
        '. A dead link in the one document that is left as the source is worse than the document ' +
        'it points at.',
    ).toEqual([]);
  });

  /* SUBSTANCJA, NIE NAGLOWKI.
   *
   * Pierwsza wersja pytala o piec naglowkow `## n. ...` — a cztery z nich staly w `DESIGN.md`
   * jeszcze przed ta fala, wiec warunek, ktory ma odroznic „wchlonieta" od „skasowana", nie
   * odroznial niczego i przechodzil takze na dokumencie z wypatroszonymi sekcjami. Nizej stoja
   * rzeczy, ktorych przed fala w `DESIGN.md` NIE BYLO i ktore byly cala trescia usunietych
   * papierow. */
  it('left their content in the files that stay', () => {
    const design = text(DESIGN);
    for (const [what, needle] of [
      ['the corner band', '--radius-pill'],
      ['the colour that means it is happening now', '--live'],
      ['the glass recipe and its escape hatch', 'prefers-reduced-transparency'],
      ['the one field the house owns', '`.field`'],
      ['the marker on an empty screen', 'data-empty'],
    ] as const) {
      expect(
        design.includes(needle),
        'the design document says nothing about ' +
          what +
          ' (' +
          needle +
          '), so deleting the wave papers deleted content instead of absorbing it',
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
