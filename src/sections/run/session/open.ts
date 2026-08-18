/* Który agent jest otwarty — jedno pole na okno, i nic poza nim.
 *
 * DLACZEGO MAGAZYN NA POZIOMIE MODUŁU, a nie `useState` w ekranie pracy. Ten sam powód, co przy
 * `runFeed` i `useRun`: ekran sekcji odmontowuje się, kiedy człowiek wejdzie do Agentów, a bieg
 * i to, co człowiek na nim otworzył, mają go przeżyć. Druga, ważniejsza połowa powodu jest
 * testowa — to repo nie ma jsdom, więc `onClick` nie odpala się w żadnym teście. Handler, który
 * trzymałby stan wewnątrz komponentu, byłby kodem, którego żadne kryterium nie umie dotknąć,
 * i to jest dokładnie ta rodzina, z której wzięły się kontrolki bez skutku (niezmiennik 16).
 * Tutaj test woła to, co woła kafelek.
 *
 * ZAMKNIĘCIE NIE JEST TU DRUGĄ KONTROLKĄ. „Wróć" na ekranie agenta i przełączenie workspace'a
 * odpowiadają na to samo pytanie („nie patrzę już na tego agenta"), więc jedno pole i jedna
 * droga. Ekran agenta, którego nie ma w liście agentów tego workspace'a, po prostu się nie
 * rysuje — rozstrzyga to montowanie, nie ten plik: identyfikator zostaje, więc powrót do
 * tamtego folderu wraca do tego samego agenta (wymóg właściciela: „nie tracę sesji").
 */

/** Podpis agenta, którego ekran jest otwarty, albo `null`. */
let opened: string | null = null;

const listeners = new Set<() => void>();

/** Otwiera ekran tego agenta. Podpis jest tym, którym agent nadaje w strumieniu. */
export function openAgent(agent: string): void {
  if (opened === agent) return;
  opened = agent;
  publish();
}

/** Zamyka ekran agenta i wraca do widoku pracy. */
export function closeAgent(): void {
  if (opened === null) return;
  opened = null;
  publish();
}

/** Kto jest otwarty. Ta sama migawka dla okna i dla renderu serwerowego. */
export function openedAgent(): string | null {
  return opened;
}

/** Powiadomienie o zmianie; oddaje funkcję, która je odwołuje. Kształt `useSyncExternalStore`. */
export function subscribeToOpenAgent(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function publish(): void {
  for (const listener of [...listeners]) listener();
}
