/* Wybór plików przez okno systemu — JEDYNE miejsce sekcji Context, które woła wtyczkę.
 *
 * Osobny plik, tak samo jak `src/sections/run/folders.ts`: `io.ts` jest krawędzią KOMEND
 * Loadouta, a to jest wtyczka systemowa. Zmieszane w jednym pliku dałyby jeden moduł
 * odpowiadający na dwa różne pytania (niezmiennik 23), a `commands-wired.test.ts` sądzi
 * cały eksport `io.ts` i wołałby okno wyboru pliku przy każdym przebiegu.
 *
 * BEZ FILTRA ROZSZERZEŃ, i to jest decyzja, nie przeoczenie. Panel, który nie daje wybrać
 * nieobsługiwanego pliku, chowa przed człowiekiem zdanie, którym Rust nazywa odmowę — a to
 * zdanie jest jedynym miejscem, z którego dowie się, czego jego plikowi brakuje (PLAN §2:
 * „Nieobsługiwany plik dostaje nazwany wynik importu").
 */
import { open as chooseFiles } from '@tauri-apps/plugin-dialog';

/**
 * Pyta człowieka o pliki. Oddaje ścieżki albo pustą listę.
 *
 * Pusta lista znaczy **anulowanie**, czyli wartość, nie błąd (niezmiennik 7). Odmowa samego
 * okna wyboru jedzie wyjątkiem, bo to jest awaria i wołający ma o niej powiedzieć zdaniem.
 */
export async function chooseFilesToAdd(): Promise<string[]> {
  const picked = await chooseFiles({
    directory: false,
    multiple: true,
    title: 'Add files to this set',
  });
  /* Wtyczka oddaje `null` przy anulowaniu, a przy `multiple: true` tablicę — sprawdzamy jednak
   * kształt, bo to jest granica z cudzym kodem, a nie nasza obietnica. */
  if (typeof picked === 'string') return [picked];
  if (!Array.isArray(picked)) return [];
  return picked.filter((one): one is string => typeof one === 'string');
}
