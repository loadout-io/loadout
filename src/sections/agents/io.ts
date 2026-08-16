/* Jedyne miejsce w sekcji Agenci, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, krawędź po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po magazynie i po formularzu. Magazyn
 * (`src/state/agents.ts`) dostaje ten moduł WSTRZYKNIĘTY jako `AgentsIo` i dzięki temu jego
 * testy podstawiają atrapę bez dotykania transportu. Zdanie „edycja kroku nie zapisuje agenta"
 * ma sens tylko wtedy, kiedy jest jedna krawędź, przez którą da się zapisać cokolwiek — dwie
 * drogi do Rusta znaczą, że asercja pilnuje jednej z nich, a zapis jedzie drugą.
 *
 * Kształt jest lustrem `AgentsIo` z `src/state/agents.ts` i tak ma zostać: funkcja dopisana
 * tutaj bez pozycji tam jest funkcją, której magazyn nie umie zawołać.
 *
 * Ciała są jeszcze puste. Szkielet ma się WCZYTAĆ i paść w czasie wykonania — moduł, którego
 * nie ma, daje „Cannot find module", czyli czerwień, której bramka nie liczy (AGENTS.md §2a).
 */
import type { Agent } from '../../state/agents';

/** Wszyscy zapisani agenci, po jednym na plik w bibliotece. */
export function list(): Promise<Agent[]> {
  throw new Error('not implemented: read the saved agents');
}

/**
 * Świeży identyfikator, wybity po stronie Rusta.
 *
 * Nie `crypto.randomUUID()`: tamto daje v4, czyli liczbę losową, a tutaj musi być v7 —
 * sortowalne po czasie [T4 §5.1]. Mennica stoi tam, gdzie v7 już jest.
 */
export function newId(): Promise<string> {
  throw new Error('not implemented: mint a fresh id');
}

/** Zapisuje definicję agenta. Duplikat to nowy PLIK, nie wiersz żyjący na ekranie. */
export function save(agent: Agent): Promise<void> {
  throw new Error('not implemented: write ' + agent.name + ' out to the library');
}

/** Usuwa agenta po identyfikatorze — stabilnym przez zmianę nazwy, w odróżnieniu od pliku. */
export function remove(id: string): Promise<void> {
  throw new Error('not implemented: drop the agent ' + id);
}
