/* Kryterium 4 dla T-25: to, co powłoka odkryła, zgadza się z tym, co naprawdę leży na dysku.
 *
 * Oczekiwany zbiór liczony jest NIEZALEŻNIE od odkrywania: przez `node:fs`, po katalogach
 * w `src/sections/`, biorąc te, które mają `index.tsx`, i przecinając je z identyfikatorami
 * z rejestru. Dwie drogi do tej samej odpowiedzi — inaczej sprawdzenie pytałoby odkrywanie
 * o zdanie na własny temat. Wzorca ścieżek nie ma tu ani razu; jest katalog i nazwa pliku,
 * bo wzorzec mieszka DOKŁADNIE RAZ, w `discoverScreens` (niezmiennik 23).
 *
 * Uczciwie o sile tego kryterium: dziś obie strony są PUSTE, bo żadna sekcja nie ma jeszcze
 * swojego `index.tsx`. Czerwień w warstwie `before` bierze się stąd, że `discoverScreens` nie
 * ma jeszcze ciała — to jest brak zachowania, nie brak modułu. Wartość jest odroczona
 * i automatyczna: w dniu, w którym pierwsza sekcja doda `index.tsx`, literówka we wzorcu
 * rozjedzie oba zbiory i to kryterium ją złapie. Bez niego zły wzorzec daje na zawsze pustą
 * mapę, czyli DOKŁADNIE ten obraz, który to zadanie usuwa — zielono i pusto.
 *
 * `expect(typeof discoverScreens()).toBe('object')` przechodzi na `() => ({})`, czyli na tej
 * właśnie awarii.
 */
import { existsSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { discoverScreens } from './screens';
import { SECTIONS } from './sections';

/** `src/sections/`, liczone od położenia TEGO pliku — nie od katalogu roboczego biegu. */
const SECTIONS_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'sections');

/** Nazwa pliku, który czyni z katalogu ekran sekcji. Konwencja z HARNESS-QUEUE.md Q-5. */
const ENTRY_FILE = 'index.tsx';

/**
 * Sekcje, które NAPRAWDĘ mają swój ekran na dysku. Brak katalogu `src/sections/` to poprawna
 * odpowiedź „na razie żadna", a nie wyjątek: dopóki pierwsze zadanie sekcji nie wyląduje,
 * ten katalog nie istnieje, a kryterium ma padać na porównaniu, nie na otwarciu ścieżki
 * (AGENTS.md §2a p. 5).
 */
function onDisk(): string[] {
  if (!existsSync(SECTIONS_DIR)) return [];
  const known = new Set<string>(SECTIONS.map((entry) => entry.id));
  return readdirSync(SECTIONS_DIR, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => known.has(name) && existsSync(join(SECTIONS_DIR, name, ENTRY_FILE)))
    .sort();
}

describe('what the shell found is what really lies in src/sections', () => {
  it('finds every section directory that has a screen file, and no others', () => {
    const expected = onDisk();
    const found = Object.keys(discoverScreens()).sort();
    expect(
      found,
      'these two sets are counted two different ways on purpose: one by walking ' +
        'src/sections with the file system, one by asking the shell what it discovered. ' +
        'They disagree exactly when the search pattern is wrong — and a wrong pattern is ' +
        'silent, because an empty answer looks like a young app rather than a broken one. ' +
        'The file system says ' +
        JSON.stringify(expected) +
        ' and the shell says ' +
        JSON.stringify(found),
    ).toEqual(expected);
  });

  it('hands back something the shell can render for each one it found', () => {
    for (const [id, found] of Object.entries(discoverScreens())) {
      expect(
        typeof found,
        id + ' was discovered, so the value under it has to be callable — the shell renders it',
      ).toBe('function');
    }
  });
});
