/* „Tidy up" — jeden przycisk, który układa kafelki z góry na dół.
 *
 * SZKIELET — ciało rzuca `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Dlaczego to nie jest kosmetyka: układ zwraca ZMIENNOPRZECINKOWE środki węzłów, więc płótno,
 * które przycina pozycje tylko w handlerze przeciągania, po każdym „Tidy up" zapisuje plik
 * z innym dziesiątym miejscem po przecinku — diff bez treści, przy każdym kliknięciu [T3 §8.2].
 * Dlatego wynik przechodzi przez tę samą siatkę co przeciąganie, a kryterium sprawdza to pętlą
 * po WSZYSTKICH krokach, nie na jednej pozycji.
 */
import type { WorkflowFile } from '../../../state/workflows';

/** Układa kroki z góry na dół: następnik stoi zawsze niżej niż jego poprzednik, a każda
 * pozycja jest całkowitą wielokrotnością `GRID`. */
export function tidyUp(_file: WorkflowFile): WorkflowFile {
  throw new Error('not implemented');
}
