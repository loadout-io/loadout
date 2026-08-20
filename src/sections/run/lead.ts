/* „Kim jest lider" po stronie okna — jeden fakt, jeden dom (niezmiennik 13).
 *
 * SZKIELET T-60. Ciała rzucają, i to jest wymóg fazy kontraktu, nie niedbalstwo: `vitest`
 * przewraca się już na ZBIERANIU brakującego importu („Cannot find module"), a to jest podpis
 * z `NOT_A_REAL_RED` — kryterium, które go dostanie, nie uruchomiło ani jednej asercji. Moduł
 * musi więc istnieć, importy muszą się rozwiązać, a padnięcie ma nastąpić na zachowaniu.
 *
 * CO TU MIESZKA, A CO NIE. Mieszka tu WYBÓR — identyfikator zapisanego agenta — i słowo, którym
 * pasek nazywa kontrolkę. Nie mieszka tu ani vendor, ani model, ani dial bezpieczeństwa: „kim
 * jest lider" ma dokładnie jedno źródło, zapisaną definicję agenta, a kopia któregokolwiek z tych
 * pól trzymana obok w stanie okna jest pierwszą rzeczą, która się rozjedzie (niezmiennik 13).
 * Okno trzyma wskazanie; kto to jest, odpowiada Rust, czytając plik.
 *
 * DLACZEGO MODUŁ, A NIE `useState` W KONTROLCE STARTU. Wybór człowieka przeżywa odmontowanie
 * ekranu: powłoka montuje dokładnie jedną sekcję (`src/App.tsx`), więc wyjście do Agentów
 * i powrót niszczyłoby stan kontrolki. Ten sam ruch i ten sam zmierzony powód, co przy
 * `./chosen-workflow.ts` i `./limits/chosen.ts` — i ten sam kształt, którego chce
 * `useSyncExternalStore`.
 */

/**
 * Etykieta dostępnościowa kontrolki lidera w pasku loadoutu.
 *
 * Stała, a nie napis wpisany w komponencie, z jednego powodu: kryterium ma ją CZYTAĆ, nie
 * przepisywać. Wpisana z palca po obu stronach byłaby zielona także wtedy, gdyby kontrolka
 * i test mówiły o dwóch różnych rzeczach — a wtedy „na pasku stoi lider" jest zdaniem o teście.
 *
 * Słowo jest z tabeli DESIGN §8: `orchestrator` jest na liście żargonu, a `lead agent` jest jego
 * zamiennikiem (niezmiennik 14). Wybór bez nazwy jest zagadką, więc kontrolka musi się nazywać.
 */
export const LEAD_LABEL = 'Lead agent';

/** Identyfikator wskazanego agenta, albo `''`, dopóki człowiek nie wybierał. */
export function lead(): string {
  throw new Error('not implemented');
}

/**
 * Zapisuje wskazanie. Identyfikatorem, nie nazwą: nazwa agenta się zmienia, `id` przeżywa
 * zmianę nazwy (T4 §5.1) i to nim posługuje się Rust, szukając definicji w bibliotece.
 */
export function setLead(_id: string): void {
  throw new Error('not implemented');
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToLead(_listener: () => void): () => void {
  throw new Error('not implemented');
}
