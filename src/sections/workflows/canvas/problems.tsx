/* Pasek nad przyciskiem Run: ile jest rzeczy do poprawienia i czy Run w ogóle działa.
 *
 * SZKIELET — ciała rzucają `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Uwagi przychodzą Z RUSTA (`workflow::check`, T-12) i są tu tylko wyświetlane. Frontend ich nie
 * wymyśla, nie tłumaczy i nie liczy po swojemu — `message` jest gotowym angielskim zdaniem
 * i to ono ląduje w `title` zablokowanego przycisku. Zablokowany Run z podpowiedzią
 * „Fix the errors first" jest przyciskiem bez wyjaśnienia: użytkownik widzi, że nie może
 * kliknąć, i nie wie dlaczego [T3 §5.3].
 *
 * Podział wagi jest całą treścią tego paska: `Problem` blokuje Run, `Warning` NIE blokuje.
 * Pasek, który liczy wszystkie uwagi i przy każdej gasi Run, zamienia ostrzeżenie o niepodłączonym
 * kroku w blokadę uruchomienia — a to jest workflow, który wolno uruchomić.
 */
import type { ReactElement } from 'react';
import type { Note } from '../../../state/workflows';

export interface RunBarProps {
  /** Uwagi z ostatniego sprawdzenia. Pusta lista znaczy „nie ma nic do poprawienia". */
  notes: Note[];
  onRun: () => void;
  /** Kliknięcie uwagi przesuwa płótno na winny krok i otwiera jego panel. */
  onFocusNote: (note: Note) => void;
}

/** Co [`focusNote`] woła. Obie funkcje przychodzą z płótna — `fitView` z `useReactFlow()`,
 * `openPanel` z ekranu — więc sama funkcja nie potrzebuje ani okna, ani hooka. */
export interface NoteFocus {
  fitView: (options: { nodes: Array<{ id: string }>; duration: number; maxZoom: number }) => void;
  openPanel: (stepId: string) => void;
}

export function RunBar(_props: RunBarProps): ReactElement {
  throw new Error('not implemented');
}

/** Przesuwa płótno na krok, którego dotyczy uwaga, i otwiera jego panel.
 *
 * Uwaga bez `stepId` dotyczy całego pliku i nie ma w co celować — wtedy nie dzieje się nic. */
export function focusNote(_note: Note, _focus: NoteFocus): void {
  throw new Error('not implemented');
}
