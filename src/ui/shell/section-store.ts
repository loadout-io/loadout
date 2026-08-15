/* Która sekcja jest otwarta. Bez routera, bez URL-i, bez historii: T8 §6.2 mówi wprost, że
 * to jest `type Section` w stanie interfejsu, a router kupowałby zależność i format
 * serializacji za zero.
 *
 * Stan mieszka TUTAJ, a nie w `src/state/ui.ts` — tamta ścieżka nie należy do żadnego zadania
 * (TASK.md, „Co to zadanie posiada").
 *
 * `go` wołamy przez `useSectionStore.getState().go(...)`, czyli spoza Reacta. To jest ten sam
 * wzorzec, którym w T-07 pisze do stanu kanał zdarzeń z Rusta [T8 §6.3] — jedna droga zapisu,
 * nie dwie.
 */
import { create } from 'zustand';
import type { Section } from '../sections';

export interface SectionState {
  section: Section;
  go: (id: Section) => void;
}

/** Sekcja, na której powłoka się otwiera: tam, gdzie dzieje się praca. */
export const FIRST_SECTION: Section = 'run';

export const useSectionStore = create<SectionState>()((set) => ({
  section: FIRST_SECTION,
  go: (id) => set({ section: id }),
}));
