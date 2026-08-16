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
 *
 * DLACZEGO PUSTY STAN NIE UŻYWA `src/ui/primitives/empty-state.tsx`. Tamten przyjmuje jedno
 * zdanie i nie ma miejsca na przycisk — powstał w T-01, kiedy nie było jeszcze czego tworzyć,
 * i jego własny nagłówek to zapowiada. DESIGN §6 wymaga tu dwóch zdań i JEDNEGO przycisku
 * podstawowego, a `src/ui/primitives/` leży poza blokiem `<!-- OWNS -->` tego zadania, więc
 * dołożenie tam przycisku byłoby zapisem poza zakresem (AGENTS.md §7). Zlanie obu wersji
 * w jedną należy do zadania, które będzie posiadało obie ścieżki.
 */
import type { ReactElement } from 'react';
import type { WorkflowEntry, WorkflowListActions } from './store';
import { newWorkflowName } from './store';
import { WorkflowTile } from './tile';

export interface WorkflowListProps {
  workflows: readonly WorkflowEntry[];
  /** O co pytamy przed usunięciem. `null` — o nic. */
  pendingDeleteId: string | null;
  /** Jeden obiekt na cały ekran; oba przyciski tworzenia dostają TEN SAM. */
  actions: WorkflowListActions;
}

/* Klasy komponentów z DESIGN §6, spisane raz. Wysokości idą po siatce 4px:
 * 36px = `h-9` (button-primary), 32px = `h-8` (button-secondary), 28px = `h-7` (button-quiet). */
const PRIMARY = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg';
const SECONDARY = 'h-8 rounded-sq border border-line-strong bg-raised px-3 text-ui text-ink';
const QUIET = 'h-7 rounded-sq border border-line px-3 text-ui text-body';
/* button-danger: jak secondary, ale bez wypełnienia. Akcja niszcząca ma być rozpoznawalna,
 * a nie najbardziej rzucająca się w oczy rzecz na ekranie (DESIGN §6). */
const DANGER = 'h-8 rounded-sq border border-fail-edge px-3 text-ui text-fail';

export function WorkflowList({
  workflows,
  pendingDeleteId,
  actions,
}: WorkflowListProps): ReactElement {
  /* JEDNA funkcja tworząca na cały ekran, i to jest cały sens niezmiennika 16: przycisk
   * w pustym stanie i przycisk w nagłówku są dwoma wejściami do jednego przepływu. Drugi
   * przepływ to drugie miejsce, w którym powstaje plik, i pierwsza okazja do rozjazdu.
   *
   * `void`, bo ekran nie ma jeszcze gdzie pokazać nieudanego zapisu — błędy plików należą
   * do T-12. To nie jest połknięty błąd: odrzucona obietnica zostawia listę bez nowej
   * pozycji, czyli w stanie zgodnym z tym, co naprawdę leży na dysku. */
  const createWorkflow = (): void => {
    void actions.create(newWorkflowName(workflows));
  };

  /* Pytamy tylko wtedy, gdy umiemy nazwać, o co pytamy. Pozycja mogła zniknąć z listy między
   * kliknięciem a renderem; pytanie „Are you sure?" bez nazwy każe zgadywać, który z trzech
   * plików za chwilę zniknie. */
  const pending =
    pendingDeleteId === null
      ? undefined
      : workflows.find((entry) => entry.workflow.id === pendingDeleteId);

  return (
    <section className="flex h-full flex-col">
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Workflows</h1>

        {/* Licznik i przycisk w nagłówku żyją tylko wtedy, gdy jest co liczyć. Przy zerze
         * to samo mówi zaproszenie niżej, a `0 saved` obok `No workflows yet.` to ten sam
         * fakt w dwóch miejscach (niezmiennik 13) — i drugi przycisk tworzenia na ekranie,
         * na którym DESIGN §6 przewiduje dokładnie jeden. */}
        {workflows.length === 0 ? null : (
          <>
            <span className="font-mono text-mono text-muted">{`${workflows.length} saved`}</span>
            <button
              data-create
              type="button"
              className={`ml-auto ${PRIMARY}`}
              onClick={createWorkflow}
            >
              ＋ Create
            </button>
          </>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {workflows.length === 0 ? (
          <div data-empty className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
              ◇
            </span>
            <p className="text-ink">No workflows yet.</p>
            <p className="text-muted">Create one to lay out the steps a run follows.</p>
            <button data-create type="button" className={PRIMARY} onClick={createWorkflow}>
              ＋ Create
            </button>
          </div>
        ) : (
          <ul className="grid grid-cols-2 gap-3">
            {workflows.map((entry) => (
              <li key={entry.path} className="flex flex-col gap-2">
                <WorkflowTile wf={entry.workflow} />

                {/* Obie kontrolki mają handler i obie wołają magazyn. Kafelek ich nie zna —
                 * akcje mieszkają tam, gdzie mieszka obiekt `actions`. */}
                <div className="flex gap-2">
                  <button
                    type="button"
                    className={QUIET}
                    onClick={() => {
                      void actions.duplicate(entry.workflow.id);
                    }}
                  >
                    Duplicate
                  </button>
                  <button
                    type="button"
                    className={QUIET}
                    onClick={() => {
                      actions.requestDelete(entry.workflow.id);
                    }}
                  >
                    Delete
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      {pending === undefined ? null : (
        <div
          data-confirm-delete
          className="fixed inset-0 z-10 flex items-center justify-center bg-bg/72 p-6"
        >
          <div className="flex w-full max-w-160 flex-col gap-4 rounded-sq border border-line-strong bg-panel p-6">
            {/* Jedno zdanie, jeden węzeł tekstowy: nazywa workflow po imieniu, mówi, co
             * znika, i mówi, co zostaje. Bieg, który się odbył, nie zależy od pliku — i to
             * jest właśnie to, czego człowiek nie wie, stojąc nad tym pytaniem. */}
            <p className="text-body text-ink">
              {`Delete "${pending.workflow.name}"? The file goes away. Runs you already did stay.`}
            </p>

            <div className="flex justify-end gap-2">
              <button
                type="button"
                className={SECONDARY}
                onClick={() => {
                  actions.cancelDelete();
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                className={DANGER}
                onClick={() => {
                  void actions.confirmDelete();
                }}
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
