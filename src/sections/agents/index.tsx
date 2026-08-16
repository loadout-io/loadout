/* Ekran sekcji Agents — SZKIELET FAZY KONTRAKTU, jeszcze bez ciała.
 *
 * Powód pustego ciała i powód, dla którego to jest `throw`, a nie pusty `<div/>`, stoją
 * w `src/sections/workflows/index.tsx` — jednym akapitem: szkielet ma się wczytać (żeby
 * odkrywanie go znalazło) i paść w czasie wykonania, a `<div/>` przepuszcza słabą asercję,
 * którą kryterium ma łapać. To jest odpowiednik `todo!()` (AGENTS.md §2a).
 *
 * CO SKŁADA FAZA WYKONAWCZA. Nagłówek z podpisem liczbowym, jedna akcja dodawania i lista
 * agentów, każdy ze swoim vendorem. Magazyn (`createAgentsStore`, T-11) jest wylądowany;
 * formularz agenta (`agent-form.tsx`, T-11) też i nie wolno pisać drugiego (niezmiennik 23).
 *
 * DWIE RZECZY, KTÓRE TU ZABOLĄ, obie zmierzone 2026-08-16:
 *
 *   1. `AgentsState` ma `load`, `duplicate` i `delete`, ale NIE MA tworzenia — mennica `newId`
 *      i `save` siedzą w `AgentsIo`, którego magazyn nie wystawia. „Dokładnie jedna kontrolka
 *      dodawania" musi więc dostać handler, który naprawdę coś robi (odsłonięcie pustego
 *      formularza jest takim handlerem), a nie `onClick={() => {}}`: kontrolka bez handlera
 *      nie wchodzi do repo (niezmiennik 16) i poprzedni prototyp ma przez to trzy martwe przyciski.
 *   2. Etykiety vendorów (`Claude Code`, `Codex`) mieszkają dziś w `VENDORS` wewnątrz
 *      `agent-form.tsx` i nie są eksportowane, a ten plik jest poza blokiem OWNS tego zadania.
 *      Druga kopia tej dwuwierszowej tabeli jest długiem — zapisz go tak, jak `src/App.tsx`
 *      zapisał swoją kopię pustego ekranu, i zgłoś człowiekowi eksport zamiast kopii.
 *
 * O migawce serwerowej zustanda przeczytaj w `src/sections/workflows/index.tsx`: magazyn
 * czytany hakiem zustanda jest w `renderToStaticMarkup` pusty niezależnie od tego, co
 * zasiał test.
 */
import type { ReactElement } from 'react';
import { createAgentsStore } from '../../state/agents';

/** Magazyn agentów — dokładnie ten, który oddaje `createAgentsStore`. */
export type AgentsStore = ReturnType<typeof createAgentsStore>;

export interface AgentsScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: AgentsStore;
}

export default function AgentsScreen(_props: AgentsScreenProps): ReactElement {
  throw new Error('not implemented: show the agents this app can run');
}
