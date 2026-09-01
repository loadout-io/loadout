/* Który krok ma otwarty swój strumień — jedno pole na okno, i nic poza nim.
 *
 * DLACZEGO MAGAZYN NA POZIOMIE MODUŁU, A NIE `useState` W EKRANIE PRACY. Ten sam powód, co przy
 * `../session/open.ts`, i obie połowy tego powodu są mierzalne. Pierwsza: ekran sekcji
 * odmontowuje się, kiedy człowiek wejdzie do Agentów, a bieg idzie dalej — wybór schowany
 * w komponencie znikałby przy każdym wyjściu z ekranu. Druga, ważniejsza: to repo nie ma jsdom,
 * więc `onClick` nie odpala się w żadnym kryterium. Handler trzymający stan wewnątrz komponentu
 * jest kodem, którego żadne kryterium nie umie dotknąć — czyli tą samą rodziną, z której wzięły
 * się kontrolki bez skutku (niezmiennik 16). Tutaj kryterium woła DOKŁADNIE to, co woła kafelek.
 *
 * KLUCZ KROKU, NIE PODPIS AGENTA. Tożsamością kafelka na obrazie jest klucz z pliku workflow
 * (`data-step`), a podpisów agenta bywa kilka na jeden krok i jeden podpis na kilka kroków. Kto
 * pod tym kluczem stoi, rozstrzyga ekran, bo to on ma plan biegu — ten plik trzyma jedno pole
 * i nie wie o biegu nic.
 *
 * ZAMKNIĘCIE JEST JEDNĄ CZYNNOŚCIĄ, nie trzema. Przycisk w szufladzie, Escape i otwarcie innego
 * kroku odpowiadają na to samo pytanie („nie patrzę już na ten krok"), więc jedno pole i jedna
 * droga. Drugi wyłącznik obok tego pierwszego jest dokładnie tym, co rozjeżdża się po cichu
 * (niezmiennik 13).
 */

/** Klucz kroku, którego strumień jest otwarty, albo `null`. */
let opened: string | null = null;

const listeners = new Set<() => void>();

/** Otwiera strumień tego kroku. Klucz jest tym z pliku workflow (`Step.id`). */
export function openStepStream(stepId: string): void {
  if (opened === stepId) return;
  opened = stepId;
  publish();
}

/** Zamyka szufladę. Wołane z przycisku, z Escape i przy zejściu biegu. */
export function closeStepStream(): void {
  if (opened === null) return;
  opened = null;
  publish();
}

/** Który krok jest otwarty. Ta sama migawka dla okna i dla renderu serwerowego. */
export function openedStepStream(): string | null {
  return opened;
}

/** Powiadomienie o zmianie; oddaje funkcję, która je odwołuje. Kształt `useSyncExternalStore`. */
export function subscribeToOpenedStepStream(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function publish(): void {
  for (const listener of [...listeners]) listener();
}
