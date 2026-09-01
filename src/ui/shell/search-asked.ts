/* „Otwórz szukanie" — prośba, którą zapisuje boczne menu, a wykonuje paleta poleceń.
 *
 * WZORZEC JEST PRZEPISANY Z `src/ui/palette/asked.ts`, ŚWIADOMIE, i z tego samego powodu, co
 * tam: kontrolka i wykonawca stoją w dwóch różnych poddrzewach, a jedyne, co ma między nimi
 * przejechać, to intencja. Menu nie wie o palecie nic poza tym, że ktoś tę prośbę odbierze —
 * import komponentu palety do menu byłby zależnością w złą stronę, a stan otwarcia palety
 * przeniesiony do menu byłby drugim miejscem na jeden fakt (niezmiennik 13).
 *
 * LICZBA, NIE FLAGA. Zapadka `true/false` jest nieodróżnialna przy drugiej prośbie: człowiek
 * zamyka paletę, klika lupę znowu i nic się nie dzieje, bo flaga dalej stoi na `true`. Ściśle
 * rosnący licznik odróżnia „poproszono" od „poproszono ZNOWU" i jest zarazem migawką w kształcie,
 * którego chce `useSyncExternalStore`.
 */

let count = 0;
const listeners = new Set<() => void>();

/** Poproś o otwarcie szukania. */
export function askForSearch(): void {
  count += 1;
  for (const listener of listeners) listener();
}

/** Ile razy dotąd poproszono. Migawka dla `useSyncExternalStore`. */
export function asked(): number {
  return count;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToAskedSearch(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
