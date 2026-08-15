/* Która sekcja jest otwarta. Bez routera, bez URL-i, bez historii: T8 §6.2 mówi wprost, że
 * to jest `type Section` w storze, a router kupowałby zależność i format serializacji za zero.
 *
 * Stan mieszka TUTAJ, a nie w `src/state/ui.ts` — tamta ścieżka nie należy do żadnego zadania
 * (TASK.md, „Co to zadanie posiada").
 *
 * SZKIELET (faza kontraktowa T-01): `go` jeszcze nie przełącza. Handler dopisuje faza
 * implementacji; kryterium kontrolek pyta store'a o wartość PO przełączeniu, więc atrapa,
 * która tylko wygląda jak handler, nie ma jak go przejść.
 */
import { create } from 'zustand';
import type { Section } from '../sections';

export interface SectionState {
  section: Section;
  go: (id: Section) => void;
}

export const useSectionStore = create<SectionState>()(() => ({
  section: 'run',
  go: () => undefined,
}));
