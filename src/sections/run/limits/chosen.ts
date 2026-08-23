/* „Ile agentów naraz" — jeden fakt, jeden dom (niezmiennik 13).
 *
 * DLACZEGO NIE `useState` W KONTROLCE STARTU, GDZIE TO STAŁO DO 2026-08-18. Ta liczba ma dwóch
 * czytelników i każdy zadaje inne pytanie. Kontrolka pyta „co człowiek wybrał", bo wysyła to
 * do `run_workflow`. Pasek kart pyta „ile miejsc ma pula", bo bez sufitu zdanie „2 in use"
 * nigdy nie zdradza, ile ich w ogóle jest (ARCHITECTURE §6a) — a `src/sections/run/index.tsx`
 * podawał tam do dziś ZERO wpisane na sztywno. Stan zamknięty w komponencie zmusza więc drugiego
 * czytelnika do zgadywania, a zgadnięta liczba na ekranie jest gorsza niż jej brak.
 *
 * PULA JEST JEDNA NA CAŁĄ APLIKACJĘ (niezmiennik 11), więc i to pole jest jedno — tak samo mówi
 * akapit „czego tu nie ma" w `src/state/workspaces.ts`. Stan modułu, nie stan komponentu, bo
 * wyjście do Agentów odmontowuje ekran, a wybór człowieka nie ma prawa wtedy wrócić do trójki.
 *
 * DLACZEGO NIE ZUSTAND. Jedna liczba, bez selektorów i bez pochodnych — magazyn dałby tu
 * warstwę, której nikt nie czyta. Kształt jest dokładnie ten, którego chce
 * `useSyncExternalStore`, i ani pola więcej; ten sam zapis stoi w `../requested.ts`.
 */
import { DEFAULT_AT_ONCE } from './at-once';

let chosen = DEFAULT_AT_ONCE;
const listeners = new Set<() => void>();

/** Ile agentów naraz ma biec — wybór człowieka, albo domyślne trzy, dopóki nie wybierał. */
export function atOnce(): number {
  return chosen;
}

/** Zapisuje wybór. Sufit i podłogę pilnuje sama kontrolka (`MIN_AT_ONCE`, `MAX_AT_ONCE`). */
export function setAtOnce(howMany: number): void {
  if (howMany === chosen) return;
  chosen = howMany;
  for (const listener of listeners) listener();
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToAtOnce(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/* SUFIT WYDATKU JEDNEGO BIEGU — jeden fakt, jeden dom (niezmiennik 13), dokładnie jak liczba
 * wyżej. Dwóch czytelników: kontrolka („co człowiek wpisał") i pasek biegu („z ilu"), więc
 * `useState` w kontrolce zmuszałby paska do zgadywania.
 *
 * `null` znaczy „bez limitu" i jest wartością domyślną: bieg, którego nikt nie ograniczył,
 * ma biec do końca, a nie stanąć na liczbie, której nikt nie wpisał.
 */

let budget: number | null = null;
const budgetListeners = new Set<() => void>();

/** Sufit wydatku tego biegu w dolarach, albo `null`, kiedy człowiek żadnego nie postawił. */
export function budgetUsd(): number | null {
  return budget;
}

/** Zapisuje sufit. `null` znaczy „bez limitu"; kontrolka pilnuje, że liczba jest dodatnia. */
export function setBudgetUsd(dollars: number | null): void {
  if (dollars === budget) return;
  budget = dollars;
  for (const listener of budgetListeners) listener();
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToBudget(listener: () => void): () => void {
  budgetListeners.add(listener);
  return () => {
    budgetListeners.delete(listener);
  };
}
