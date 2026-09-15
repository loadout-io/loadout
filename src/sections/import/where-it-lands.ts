/* DOKĄD ten import zapisze — jedyne zdanie, którego to okno nie mówiło, a które rozstrzyga,
 * czy człowiek znajdzie potem swoje pliki.
 *
 * # Co jest naprawiane (2026-09-15)
 *
 * Import CZYTA folder z pola „Project folder", a ZAPISUJE do biblioteki projektu aktywnego
 * w oknie: `apply_setup` bierze osobny argument `folder` (`src/sections/import/io.ts`), a Rust
 * robi z niego `<folder>/.loadout`. To są dwa różne pytania i dwie różne odpowiedzi — do dziś
 * ekran wypisywał wyłącznie pierwszą.
 *
 * Rozjazd nie jest teoretyczny i nie wymaga niczyjego błędu. Wybór aktywnego projektu NIE
 * PRZEŻYWA restartu (`src/state/workspaces.ts`, `pick()` oddaje `all[0]`, z powodem zapisanym
 * przy tej funkcji), a pierwszy na liście właściciela to `inne-i-zadania`. Po świeżym starcie
 * skan czyta więc `urc-monorepo`, a pliki lądują w zupełnie innym projekcie i nikt tego nie
 * mówi ani przed, ani po.
 *
 * # Czego tu NIE MA i dlaczego
 *
 * Zmiany celu. To okno ma ten fakt NAZWAĆ, a nie nim sterować: trwałość wyboru aktywnego
 * projektu jest osobną robotą, a drugie miejsce, w którym wybiera się projekt, byłoby drugim
 * źródłem prawdy o tym, gdzie pracujemy (niezmiennik 13).
 *
 * Skrótu ścieżki. Człowiek rozstrzyga tu, CZY to jest ten projekt, więc nazwa bez folderu nie
 * wystarcza: dwa zakresy wolno nazwać tak samo, a folder jest kluczem wpisu.
 */
import type { Workspace } from '../../state/workspaces';

/**
 * Co się stanie z rozmową, która JUŻ stoi otwarta.
 *
 * ZMIERZONE 2026-09-15: sterownik lidera powstaje raz, przy starcie rozmowy, i to wtedy
 * dostaje `--mcp-config`; kolejne wiadomości idą tą samą drogą. Człowiek zaimportuje
 * połączenia, napisze do otwartego lidera, dalej nie będzie miał narzędzi — i to jest
 * dokładnie ta chwila, w której stwierdzi, że produkt nie działa. Zdanie stoi przy wyniku,
 * bo tam człowiek patrzy zaraz po imporcie, i mówi mu następny ruch, a nie samą przyczynę.
 */
export const A_NEW_CONVERSATION =
  'An agent that is already talking keeps the connections it started with, so start a new ' +
  'conversation to use anything this import added.';

/**
 * Zdanie PRZED zapisem: co czytamy i dokąd to pójdzie.
 *
 * Trzy stany świata, trzy zdania. Bez aktywnego projektu okno mówi to wprost — pusta ścieżka
 * w nawiasie czyta się jak usterka, a odpowiedzią na „gdzie to trafi" jest wtedy „nigdzie".
 * Kiedy czytany folder JEST folderem docelowego projektu, drugie zdanie byłoby powtórzeniem
 * tego, co stoi w polu nad nim. Różnica wymienia oba miejsca, bo dopiero wtedy w ogóle widać,
 * że są dwa.
 */
export function landingSays(reading: string, target: Workspace | null): string {
  if (target === null) return 'No project is open, so these files have nowhere to go yet.';
  const into = `These files go into ${target.name} (${target.folder}).`;
  const from = reading.trim();
  if (from === '' || from === target.folder) return into;
  return `Reading ${from}. ${into}`;
}

/** Zdanie PO zapisie: ile plików i — nadal — w którym projekcie one leżą. */
export function importedSays(count: number, target: Workspace | null): string {
  const files = `${String(count)} files imported`;
  if (target === null) return `${files}.`;
  return `${files} into ${target.name} (${target.folder}).`;
}
