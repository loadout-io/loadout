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
import type { DefinitionProblem } from '../../../state/library';
import { problemSays } from '../../../state/library';
import type { Library, WorkflowEntry, WorkflowListActions } from './store';
import { newWorkflowName } from './store';
import { WorkflowTile } from './tile';

export interface WorkflowListProps {
  workflows: readonly WorkflowEntry[];
  problems?: readonly DefinitionProblem[];
  /**
   * Co wiadomo o katalogu — patrz [`Library`]. Domyślnie `read`, bo komponent jest STEROWANY
   * i wołający, który o tym nie mówi, mówi „mam już odpowiedź".
   */
  library?: Library;
  /** Zdanie dysku przy `library === 'unreadable'`. Ekran go nie wymyśla. */
  refusal?: string | null;
  /** O co pytamy przed usunięciem. `null` — o nic. */
  pendingDeleteId: string | null;
  /** Jeden obiekt na cały ekran; oba przyciski tworzenia dostają TEN SAM. */
  actions: WorkflowListActions;
  /**
   * Otwarcie workflow w edytorze. Bez tego płótno (`canvas.tsx`) jest kodem, do którego nie
   * prowadzi ani jedno kliknięcie — a taki komponent ma testy i nie ma użytkowników.
   */
  onOpen: (path: string) => void;
}

/* 2026-08-31 — CZTERY STAŁE Z KLASAMI ZNIKŁY. Stały tu `PRIMARY`, `SECONDARY`, `QUIET`
 * i `DANGER`, czyli cztery przepisane ręcznie geometrie przycisków z DESIGN §6. Od dziś nosi
 * je `theme.css` pod nazwami `.btn-primary`, `.btn`, `.btn-quiet` i `.btn-danger` — razem ze
 * stanami (`:hover`, `:active`, `:focus-visible`, `:disabled`), których żadna z tych stałych
 * nie miała. Kopia decyzji w komponencie jest tym, przez co ten przycisk miał w repo pięć
 * zapisów jednej wysokości; nazwa ma jeden. */

export function WorkflowList({
  workflows,
  problems = [],
  library = 'read',
  refusal = null,
  pendingDeleteId,
  actions,
  onOpen,
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
  const hasAnything = workflows.length > 0 || problems.length > 0;

  /* CO POKAZUJE TEN EKRAN — jedna odpowiedź, cztery możliwe, policzona w jednym miejscu.
   *
   * 2026-08-31, zgłoszenie właściciela: pytanie brzmiało dotąd „czy lista jest pusta", czyli
   * jeden bit tam, gdzie stany są trzy. Zaproszenie było odpowiedzią domyślną, więc padało
   * także wtedy, gdy katalogu nikt jeszcze nie czytał ORAZ wtedy, gdy nie dało się go
   * przeczytać. Od dziś zaproszenie jest jednym z czterech wyjść, a nie tłem. */
  const shows: 'list' | 'reading' | 'unreadable' | 'empty' = hasAnything
    ? 'list'
    : library === 'reading'
      ? 'reading'
      : library === 'unreadable'
        ? 'unreadable'
        : 'empty';

  return (
    <section className="flex h-full flex-col">
      {/* `.screen-head` NIE MA TŁA z rozmysłu (theme.css): jest chrome, więc materiał bierze
          z klasy materiału obok — inaczej `prefers-reduced-transparency` przestałby go dotyczyć. */}
      <header className="screen-head glass">
        <h1 className="text-title text-ink">Workflows</h1>

        {/* Licznik i przycisk w nagłówku żyją tylko wtedy, gdy jest co liczyć. Przy zerze
         * to samo mówi zaproszenie niżej, a `0 saved` obok `No workflows yet.` to ten sam
         * fakt w dwóch miejscach (niezmiennik 13) — i drugi przycisk tworzenia na ekranie,
         * na którym DESIGN §6 przewiduje dokładnie jeden. */}
        {shows !== 'list' ? null : (
          <>
            {workflows.length === 0 ? null : (
              <span className="value">{`${workflows.length} saved`}</span>
            )}
            {problems.length === 0 ? null : (
              <span className="value" data-tone="fail">{`${problems.length} need attention`}</span>
            )}
            <button
              data-create
              type="button"
              className="btn-primary ml-auto"
              onClick={createWorkflow}
            >
              ＋ Create
            </button>
          </>
        )}
      </header>

      <div className="screen-body">
        {shows === 'reading' ? (
          /* CZY TO TRWA (DESIGN §7). Kropki, nie krążek: krążek nie mówi ani co trwa, ani ile
             zostało. Zdanie niesie treść, kropki niosą „jeszcze idzie", więc są `aria-hidden`. */
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <p className="text-ink">Reading the workflows you have saved…</p>
            <span data-reading className="thinking text-muted">
              <span aria-hidden />
              <span aria-hidden />
              <span aria-hidden />
            </span>
          </div>
        ) : shows === 'unreadable' ? (
          /* NIE UDAŁO SIĘ PRZECZYTAĆ. Do 2026-08-31 ten stan nie istniał wcale: `load()` nie miał
             `catch`, więc odmowa ginęła w nieobsłużonej obietnicy, a ekran zapraszał do tworzenia
             pliku w katalogu, którego nie da się przeczytać. Zaproszenia tu nie ma z rozmysłu. */
          <div
            data-refusal
            role="alert"
            className="fade-in flex h-full flex-col items-center justify-center gap-3 px-4 text-center"
          >
            <span className="mark">◇</span>
            {/* `text-fail` klasą, nie `data-tone`: ton maluje `.lead` i `.value`, a to zdanie
                żadnej z tych ról nie nosi (2026-08-31). */}
            <p className="text-fail">{refusal}</p>
            <p className="lead">
              Nothing is lost. Open that folder, put it right, and come back to this section.
            </p>
          </div>
        ) : shows === 'empty' ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="mark">◇</span>
            {/* `data-empty` siedzi na elemencie, ktory niesie SAMO zdanie — nie na opakowaniu
                z glifem, zaproszeniem i przyciskiem. Tak mowi o sobie `src/App.tsx` i tak robia
                Agents, Skills i Memory; do 2026-08-19 ta jedna sekcja trzymala znacznik wyzej,
                wiec kazda wyrocznia czytajaca ten znacznik dostawala tu cztero-czlonowy akapit
                zamiast zdania. */}
            <p data-empty className="text-ink">
              No workflows yet.
            </p>
            <p className="lead">Create one to lay out the steps a run follows.</p>
            <button data-create type="button" className="btn-primary" onClick={createWorkflow}>
              ＋ Create
            </button>
          </div>
        ) : (
          <ul className="grid grid-cols-2 gap-3">
            {workflows.map((entry) => (
              <li key={entry.path} className="flex flex-col gap-2">
                {/* KAFELEK JEST OTWARCIEM (makieta: `<button class="tile" data-go="flows">`).
                 * Do 2026-08-18 stał tu `<article>` i osobny przycisk `Open` pod kartą —
                 * czyli trzy szare przyciski pod każdą pozycją i ani jednego miejsca, w które
                 * kliknięcie robi to, czego człowiek się spodziewa. */}
                {/* PLIK, KTÓREGO NIE DA SIĘ PRZECZYTAĆ, DOSTAJE ZDANIE — nie wywraca listy.
                    Zmierzone w przeglądarce 2026-08-18: „TypeError: Cannot read properties of
                    undefined (reading 'description')" w `tile.tsx`. Sygnatura mówi
                    `workflow: WorkflowFile`, ale po drugiej stronie granicy nie ma typów, jest
                    JSON — a jeden zepsuty plik w katalogu zabierał CAŁĄ sekcję.

                    Zdanie, nie pominięcie: plik odfiltrowany w ciszy znika z ekranu, a człowiek
                    widzi katalog, w którym „nie ma" workflow, który tam leży. Bez `Open`, bo nie
                    ma czego otworzyć — kontrolka bez skutku jest gorsza niż jej brak
                    (niezmiennik 16). Usunąć go dalej można: `Delete` stoi niżej i działa na
                    ścieżce, nie na treści. */}
                {Array.isArray(entry.workflow?.steps) ? (
                  <WorkflowTile
                    wf={entry.workflow}
                    place={entry.place}
                    onOpen={() => {
                      onOpen(entry.path);
                    }}
                  />
                ) : (
                  <div data-unreadable={entry.path} data-tone="fail" className="card enter">
                    <p className="text-heading text-ink">{entry.path}</p>
                    <p className="lead">
                      This file is not a workflow Loadout can read. Open it and check it, or remove
                      it below.
                    </p>
                  </div>
                )}

                {/* Obie kontrolki mają handler i obie wołają magazyn. Kafelek ich nie zna —
                 * akcje mieszkają tam, gdzie mieszka obiekt `actions`. Stoją POZA kafelkiem,
                 * a nie w nim: przycisk w przycisku jest markupem, w którym przeglądarka sama
                 * decyduje, które kliknięcie wygrało. */}
                <div className="flex gap-2">
                  <button
                    type="button"
                    className="btn-quiet"
                    onClick={() => {
                      void actions.duplicate(entry.workflow.id);
                    }}
                  >
                    Duplicate
                  </button>
                  <button
                    type="button"
                    className="btn-quiet"
                    onClick={() => {
                      actions.requestDelete(entry.workflow.id);
                    }}
                  >
                    Delete
                  </button>
                </div>
              </li>
            ))}

            {problems.map((problem) => (
              <li
                key={problem.fileName}
                data-definition-problem={problem.fileName}
                data-tone="fail"
                className="card enter flex flex-col gap-2"
              >
                <p className="text-heading text-ink">{problem.fileName}</p>
                <p className="lead">{problemSays(problem)}</p>
              </li>
            ))}

            {/* Kafelek tworzenia z makiety (linia 651, `border-style:dashed`). Ta sama funkcja,
             * co oba pozostałe wejścia — jeden przepływ, trzy wejścia (niezmiennik 16).
             * NIE nosi `data-tile`: `data-tile` znaczy „workflow, który leży na dysku", a licznik
             * tych znaczników jest tym, czym kryteria mierzą zawartość katalogu. */}
            <li className="flex flex-col gap-2">
              <button
                data-create
                type="button"
                onClick={createWorkflow}
                data-tone="empty"
                data-interactive
                /* `bg-transparent` PO prymitywie, bo to jedyna rzecz, którą ten kafelek ma inaczej
                   niż karta: nie jest plikiem, więc nie ma wyglądać jak wypełniona pozycja listy. */
                className="card flex h-full flex-col gap-2 bg-transparent text-left text-muted hover:text-ink"
              >
                <span className="text-heading">＋ Create a workflow</span>
                <span className="text-body">Start from an empty canvas.</span>
              </button>
            </li>
          </ul>
        )}
      </div>

      {pending === undefined ? null : (
        <div
          data-confirm-delete
          /* DESIGN §6 `modal`: „bez animacji wjazdu poza `opacity`". Sprężyny tu nie ma
             z rozmysłu — rzecz, która przykrywa cały ekran, ma się pojawić, a nie wskoczyć. */
          className="fade-in fixed inset-0 z-10 flex items-center justify-center bg-bg/72 p-6"
        >
          <div className="flex w-full max-w-160 flex-col gap-4 rounded-lg border border-line-strong bg-overlay p-6">
            {/* Jedno zdanie, jeden węzeł tekstowy: nazywa workflow po imieniu, mówi, co
             * znika, i mówi, co zostaje. Bieg, który się odbył, nie zależy od pliku — i to
             * jest właśnie to, czego człowiek nie wie, stojąc nad tym pytaniem. */}
            <p className="text-body text-ink">
              {`Delete "${pending.workflow.name}"? The file goes away. Runs you already did stay.`}
            </p>

            <div className="flex justify-end gap-2">
              <button
                type="button"
                className="btn"
                onClick={() => {
                  actions.cancelDelete();
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn-danger"
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
