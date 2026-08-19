/* „Który workflow puszczamy" — jeden fakt, jeden dom (niezmiennik 13).
 *
 * DLACZEGO NIE `useState` W KONTROLCE STARTU, GDZIE TO STAŁO DO 2026-08-19. Wybór człowieka
 * przeżywa **odmontowanie ekranu**. Powłoka montuje dokładnie jedną sekcję (`src/App.tsx`), więc
 * wyjście do Agentów i powrót niszczyło stan kontrolki — po powrocie na ekran pracy w liście stał
 * znowu domyślny „pierwszy, który ma kroki", a nie to, co człowiek wybrał chwilę wcześniej.
 * To jest ten sam ruch i to samo zdanie, co przy `./limits/chosen`, gdzie ten sam defekt opisano
 * dzień wcześniej dla liczby „ile naraz".
 *
 * DLACZEGO KOMENTARZ JEST KRÓTSZY, NIŻ BYŁ. Pierwsza wersja tego pliku (2026-08-19) uzasadniała go
 * DRUGIM czytelnikiem: wiersz wejścia miał czytać ten wybór, żeby proza bez ukośnika uruchamiała
 * workflow, na który człowiek patrzy. Tamta droga została zastąpiona inną decyzją — proza odsyła
 * dziś do `/run` — więc czytelnik jest jeden i tak trzeba to napisać. Argument o odmontowaniu
 * ekranu stoi sam i jest zmierzony; argument o drugim czytelniku byłby dziś nieprawdą, a proza,
 * która powołuje się na kod, którego nie ma, wprowadza w błąd następnego czytającego.
 *
 * PUSTY NAPIS ZNACZY „CZŁOWIEK JESZCZE NIE WYBIERAŁ", a nie „żaden" — kontrolka Startu rozwija
 * to na „pierwszy workflow, który MA kroki" i ta reguła mieszka u niej, bo to ona zna listę.
 *
 * DLACZEGO NIE ZUSTAND. Jedno pole, bez selektorów i bez pochodnych — magazyn dałby tu warstwę,
 * której nikt nie czyta. Kształt jest ten, którego chce `useSyncExternalStore`, tak samo jak
 * w `./requested.ts` i `./limits/chosen.ts`.
 */

let chosen = '';
const listeners = new Set<() => void>();

/** Nazwa pliku wybranego workflow, albo `''`, dopóki człowiek nie wybierał. */
export function chosenWorkflow(): string {
  return chosen;
}

/** Zapisuje wybór. Nazwą pliku, nie nazwą własną: tym samym kluczem posługuje się `launchRun`. */
export function setChosenWorkflow(path: string): void {
  if (path === chosen) return;
  chosen = path;
  for (const listener of listeners) listener();
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToChosenWorkflow(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
