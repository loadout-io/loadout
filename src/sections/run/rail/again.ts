/* Powtórzenie jednego kroku, wywołane z kafelka w liście agentów.
 *
 * 2026-08-23 — ZGŁOSZENIE WŁAŚCICIELA: „możemy zrobić restart/re-run danego kroku dowolnego
 * agenta, tego teraz nie ma". Powód jest z rachunku: jego bieg trwał 48 minut i padł na ostatnim
 * sprawdzeniu z przyczyny środowiskowej, a poprawienie tego jednego kroku wymagało puszczenia
 * całej dziesiątki od zera.
 *
 * OSOBNY PLIK, nie ciało handlera w `rail.tsx`: tamten plik jest MONTAŻEM i wszystko, co
 * decyduje, mieszka obok niego w czystych funkcjach — inaczej „co dokładnie robi ten przycisk"
 * nie da się zapytać bez przeglądarki, której to repo nie ma.
 */
import { activeWorkspace } from '../../../state/workspaces';
import { useRun } from '../../../state/run';
import { atOnce as atOnceNow } from '../limits/chosen';
import { rerunStep } from '../io';

/** Zdanie, kiedy nie wiadomo, którym plikiem ten bieg poszedł. */
const NO_FILE =
  'Loadout does not know which workflow this run came from, so it cannot run a step again.';

/**
 * Powtarza krok o tym kluczu — w tym workspace, tym samym plikiem, co ostatni bieg.
 *
 * Odmowa i zdanie o zmienionym pliku wracają tą samą drogą, którą idzie każda inna odpowiedź
 * tego ekranu: przez `said` magazynu biegu. Cicha porażka wygląda tu dokładnie jak przycisk,
 * który nic nie robi.
 */
export function runStepAgain(step: string, say: (text: string) => void): void {
  const run = useRun.getState();
  if (run.fileName === '') {
    say(NO_FILE);
    return;
  }
  void rerunStep(run.fileName, step, atOnceNow(), activeWorkspace()?.folder ?? null)
    .then((said) => {
      /* Zdanie przychodzi TYLKO wtedy, gdy dzisiejszy plik różni się od tego, który wtedy biegł.
       * „To samo jeszcze raz" nie potrzebuje komentarza; „to samo z twoją poprawką" potrzebuje. */
      if (said !== null) say(said);
    })
    .catch((error: unknown) => {
      say(error instanceof Error ? error.message : String(error));
    });
}
