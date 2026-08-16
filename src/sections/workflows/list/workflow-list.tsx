/* Ekran listy workflow (makieta `docs/mockup/index.html:636-656`): nagłówek `Workflows`,
 * licznik `3 saved`, przycisk `＋ Create`, siatka kafelków.
 *
 * Komponent jest STEROWANY — stan i akcje przychodzą propsami, więc każde kryterium da się
 * postawić bez zdarzenia myszy i bez DOM-u (w repo nie ma `jsdom` ani
 * `@testing-library/react`, a `package.json` jest na liście `DENIED` w checks/quick-scope.sh).
 *
 * Trzy rzeczy, których kryteria pilnują w markupie:
 *
 *   `data-create`   przycisk tworzenia. W pustym stanie jest DOKŁADNIE JEDEN przycisk na
 *                   całym ekranie i to jest ten. Obie ścieżki tworzenia wołają ten sam
 *                   `actions.create` — drugi przepływ to drugie miejsce, w którym powstaje
 *                   plik (niezmiennik 16).
 *   `data-empty`    zaproszenie przy zerze workflow: `No workflows yet.` plus jedno zdanie
 *                   instrukcji (DESIGN §6). Pusty ekran to zaproszenie do działania, nie
 *                   komunikat o braku danych — żadnych nagłówków tabeli i żadnej pustej siatki.
 *   `data-confirm-delete`
 *                   pytanie przed usunięciem. Pojawia się przy `pendingDeleteId`, znika po
 *                   `cancelDelete()` i po `confirmDelete()`. Zdanie nazywa workflow po imieniu
 *                   i mówi, co znika.
 *
 * Licznik `N saved` jest wyliczany z `workflows.length` (niezmiennik 13). Osobne pole
 * w stanie rozjeżdża się przy pierwszym usunięciu i nikt tego nie zauważa, bo ekran dalej
 * wygląda poprawnie.
 */
import type { ReactElement } from 'react';
import type { WorkflowEntry, WorkflowListActions } from './store';

export interface WorkflowListProps {
  workflows: readonly WorkflowEntry[];
  /** O co pytamy przed usunięciem. `null` — o nic. */
  pendingDeleteId: string | null;
  /** Jeden obiekt na cały ekran; oba przyciski tworzenia dostają TEN SAM. */
  actions: WorkflowListActions;
}

/* Szkielet fazy kontraktu — odpowiednik `todo!()`, z tego samego powodu, co w `tile.tsx`. */
export function WorkflowList(_props: WorkflowListProps): ReactElement {
  throw new Error('not implemented');
}
