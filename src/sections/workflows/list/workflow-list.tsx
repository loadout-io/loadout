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
 * ── 2026-08-31, KOMPOZYCJA: EKRAN MA JEDNEGO BOHATERA ────────────────────────────────────
 *
 * Zmierzone na zrzucie z produktu: 55% wysokości ekranu było pustką pod kartami, a największą
 * rzeczą na nim był rząd jednakowych szarych przycisków. Siatka miała sześć identycznych
 * kafelków i ani jednego miejsca, na którym oko miałoby się zatrzymać.
 *
 * Trzy rzeczy zmieniły się razem i żadna z nich nie działa bez pozostałych:
 *
 *   BOHATER. Dokładnie jedna karta jest największa i bierze obie kolumny — ta, którą człowiek
 *   uruchamiał NAJPÓŹNIEJ (`theOneToRunNext` niżej). To ona, i tylko ona, niesie jedyną na
 *   ekranie czynność główną (`.btn-primary`) i tylko ona pokazuje nazwy swoich kroków.
 *
 *   `＋ Create` SCHODZI Z PIERWSZEGO GŁOSU, kiedy jest już co uruchamiać. Czynność główna jest
 *   jedna na ekran, a na liście workflow jest nią uruchomienie, nie założenie siódmego pliku.
 *   W pustym stanie `Create` zostaje `.btn-primary`, bo wtedy naprawdę nie ma nic innego do
 *   zrobienia — jedna zasada, dwie różne odpowiedzi.
 *
 *   PORZĄDKI WCHODZĄ DO KARTY. `Duplicate` i `Delete` stały POD kartą, poza jej ramką, przy
 *   każdej pozycji; dziś leżą w stopce karty i widać je pod kursorem albo pod ogniskiem
 *   klawiatury. Szczegół i powód, dla którego to `opacity`, a nie `hidden` — w `./tile.tsx`.
 *
 * Czego tu ŚWIADOMIE NIE MA: kafelków ze statystykami, których nikt nie zamawiał, i trzeciej
 * kolumny. Przy sześciu plikach trzecia kolumna robi z kart wizytówki i DOKŁADA pustki.
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
import type { RunsBehindIt } from './history';
import { lastOneRun } from './history';
import { WorkflowTile } from './tile';

export interface WorkflowListProps {
  workflows: readonly WorkflowEntry[];
  problems?: readonly DefinitionProblem[];
  /**
   * Co każdy workflow ma za sobą, ułożone pod jego nazwą (`./history.ts`).
   *
   * Domyślnie PUSTA mapa, bo komponent jest sterowany: wołający, który o biegach nie mówi,
   * mówi „nie wiem nic", a nie „nie było ich". Skutek na ekranie jest ten sam — karta milczy
   * o historii — i to jest właściwa odpowiedź na obie te niewiedze.
   */
  runs?: ReadonlyMap<string, RunsBehindIt>;
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
  /**
   * Uruchomienie workflow spod tej ścieżki.
   *
   * WYMAGANY, tak samo jak `onOpen`, i to nie jest surowość dla surowości: `Run` jest od
   * 2026-08-31 czynnością główną tego ekranu, a props opcjonalny znaczy, że wołający może ją
   * zgubić bez jednego błędu kompilacji — czyli że ekran wraca do stanu, w którym uruchomienie
   * workflow wymaga wejścia do edytora, i nikt się o tym nie dowie.
   */
  onRun: (path: string) => void;
}

/* 2026-08-31 — CZTERY STAŁE Z KLASAMI ZNIKŁY. Stały tu `PRIMARY`, `SECONDARY`, `QUIET`
 * i `DANGER`, czyli cztery przepisane ręcznie geometrie przycisków z DESIGN §6. Od dziś nosi
 * je `theme.css` pod nazwami `.btn-primary`, `.btn`, `.btn-quiet` i `.btn-danger` — razem ze
 * stanami (`:hover`, `:active`, `:focus-visible`, `:disabled`), których żadna z tych stałych
 * nie miała. Kopia decyzji w komponencie jest tym, przez co ten przycisk miał w repo pięć
 * zapisów jednej wysokości; nazwa ma jeden. */

/**
 * Ta jedna pozycja, którą ekran stawia największą — albo `null`, kiedy nie ma czego stawiać.
 *
 * 2026-08-31, ZASADA NADRZĘDNA KOMPOZYCJI: ekran ma jednego bohatera i jest nim to, po co
 * człowiek tu przyszedł. Na liście workflow to jest workflow, który zaraz uruchomi — a ten
 * da się wskazać, a nie zgadnąć: to ten, który uruchamiał NAJPÓŹNIEJ. Dopóki nie uruchomił
 * żadnego, pierwsze miejsce bierze pierwsza czytelna pozycja, bo „zacznij tutaj" jest
 * uczciwszą odpowiedzią niż sześć jednakowych kart.
 *
 * Pozycji, której nie da się przeczytać, nie stawiamy nigdy: karta bohatera niesie `Run`,
 * a plik bez kroków nie ma czego uruchomić.
 */
function theOneToRunNext(
  workflows: readonly WorkflowEntry[],
  runs: ReadonlyMap<string, RunsBehindIt>,
): WorkflowEntry | null {
  const readable = workflows.filter((entry) => Array.isArray(entry.workflow?.steps));
  const latest = lastOneRun(runs);
  const ran = latest === null ? undefined : readable.find((one) => one.workflow.name === latest);
  return ran ?? readable[0] ?? null;
}

export function WorkflowList({
  workflows,
  problems = [],
  runs = new Map<string, RunsBehindIt>(),
  library = 'read',
  refusal = null,
  pendingDeleteId,
  actions,
  onOpen,
  onRun,
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

  /* Bohater ekranu i cała reszta. Kolejność reszty zostaje TAKA, JAKA PRZYSZŁA (magazyn sortuje
   * po nazwie): lista, która przestawia się sama po każdym biegu, jest listą, w której nie da
   * się zapamiętać, gdzie co leży. Wyjęta jest dokładnie jedna pozycja. */
  const hero = theOneToRunNext(workflows, runs);
  const rest = hero === null ? workflows : workflows.filter((one) => one.path !== hero.path);

  /* JEDNA POZYCJA LISTY, dwa możliwe kształty. Ta funkcja istnieje, żeby bohater i cała reszta
   * powstawały z TEGO SAMEGO kodu: dwa osobne zapisy tej samej karty rozjeżdżają się przy
   * pierwszej zmianie i tylko jeden z nich dostaje poprawkę.
   *
   * PLIK, KTÓREGO NIE DA SIĘ PRZECZYTAĆ, DOSTAJE ZDANIE — nie wywraca listy. Zmierzone
   * w przeglądarce 2026-08-18: „TypeError: Cannot read properties of undefined (reading
   * 'description')" w `tile.tsx`. Sygnatura mówi `workflow: WorkflowFile`, ale po drugiej
   * stronie granicy nie ma typów, jest JSON — a jeden zepsuty plik zabierał CAŁĄ sekcję.
   * Bez `Open` i bez `Run`, bo nie ma czego otworzyć ani uruchomić (niezmiennik 16); `Delete`
   * zostaje, bo działa na ścieżce, nie na treści, i jest jedynym sposobem, żeby ten plik
   * z listy zszedł. */
  const entryOnTheList = (entry: WorkflowEntry, first: boolean): ReactElement =>
    Array.isArray(entry.workflow?.steps) ? (
      <WorkflowTile
        key={entry.path}
        wf={entry.workflow}
        place={entry.place}
        runs={runs.get(entry.workflow.name)}
        first={first}
        onOpen={() => {
          onOpen(entry.path);
        }}
        onRun={() => {
          onRun(entry.path);
        }}
        onDuplicate={() => {
          void actions.duplicate(entry.workflow.id);
        }}
        onDelete={() => {
          actions.requestDelete(entry.workflow.id);
        }}
      />
    ) : (
      <li
        key={entry.path}
        data-unreadable={entry.path}
        data-tone="fail"
        className="card enter flex flex-col gap-2 p-0"
      >
        <div className="flex flex-col gap-2 p-3">
          <p className="text-subhead text-ink">{entry.path}</p>
          <p className="lead">
            This file is not a workflow Loadout can read. Open it and check it, or remove it here.
          </p>
        </div>
        <div className="flex justify-end border-t border-t-line px-3 py-2">
          <button
            type="button"
            className="btn-danger"
            onClick={() => {
              actions.requestDelete(entry.workflow.id);
            }}
          >
            Delete
          </button>
        </div>
      </li>
    );

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
            {/* 2026-08-31 — `＋ Create` SCHODZI Z PIERWSZEGO GŁOSU, kiedy jest już co uruchamiać.
                Głośności są trzy i czynność główna jest JEDNA na ekran; na liście workflow jest
                nią uruchomienie tego, który leży na wierzchu, a nie założenie siódmego pliku.
                W pustym stanie (niżej) `Create` zostaje `.btn-primary`, bo wtedy naprawdę nie ma
                nic innego do zrobienia — ta sama zasada, dwie różne odpowiedzi. */}
            <button data-create type="button" className="btn ml-auto" onClick={createWorkflow}>
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
             pliku w katalogu, którego nie da się przeczytać. Zaproszenia tu nie ma z rozmysłu.

             ── 2026-08-31, DRUGIE ZGŁOSZENIE WŁAŚCICIELA: „rozjebałeś tę sekcję" ──────────────
             Ten stan wjechał rano i tego samego wieczora zamienił całą sekcję w czarny prostokąt
             z dwoma zdaniami. Zmierzone, zanim cokolwiek tu ruszyłem:

               `~/.loadout/workflows/` trzyma PIĘĆ czytelnych plików, a `~/.loadout/loadout.log`
               ma dziesiątki wierszy `Operation not permitted (os error 1)` na ścieżkach pod
               `~/Desktop/...`. Ta sama powłoka, w której to mierzyłem, czyta te katalogi bez
               problemu — odmawia się WYŁĄCZNIE aplikacji. To jest TCC, czyli zgoda systemu,
               a nie zepsuty plik.

             Wynikały z tego dwie osobne wady i obie mieszkały w tych kilkunastu liniach:

             ZDANIE WYSYŁAŁO W ZŁE MIEJSCE. „Open that folder, put it right" mówi, że coś jest
             nie tak z ZAWARTOŚCIĄ. Przy odmowie systemu w folderze nie ma czego poprawiać:
             człowiek otwiera go w Finderze, widzi swoje pliki na miejscu i wraca do tego samego
             ekranu. Naprawą jest przełącznik w Ustawieniach systemowych, a tego zdania nie da
             się zgadnąć z „Operation not permitted".

             DWÓCH PRZYCZYN NIE DA SIĘ TU ROZRÓŻNIĆ i to jest stan drutu, nie wybór tego pliku.
             `list_workflows` odrzuca `Result<_, String>` (`src-tauri/src/ipc.rs`), czyli sam
             NAPIS z `LoadError`. Kategorii nie ma: `DefinitionProblemKind` jedzie wyłącznie
             PRZY PLIKU, a odmowa całego listowania nie niesie jej wcale. Zgadywanie przyczyny
             z treści błędu byłoby parsowaniem cudzego komunikatu — więc zdanie niżej NAZYWA OBIE
             możliwości zamiast wybierać jedną, i pod każdą z nich stoi ruch, który naprawdę
             działa. Brak nośnika jest zgłoszony; do dnia, w którym powstanie, to jest maksimum
             prawdy, jakie ten ekran ma.

             KONTROLKA. Poprzednia wersja nie miała ani jednej: człowiek, który właśnie nadał
             dostęp, nie miał czym poprosić o drugi odczyt. Jedyną drogą było wyjście z sekcji
             i powrót, bo dopiero wtedy efekt w `../index.tsx` woła `load()`. `Try again` woła
             DOKŁADNIE to samo `load()` — jeden odczyt katalogu, dwa wejścia (niezmiennik 16).
             Jedna, nie trzy: „wybierz inny folder" mieszka w przełączniku zakresów i drugie
             wejście do niego byłoby drugim miejscem, w którym mieszka odpowiedź na pytanie
             „gdzie pracujemy" (niezmiennik 13). */
          <div
            data-refusal
            role="alert"
            className="fade-in flex h-full flex-col items-center justify-center gap-3 px-4 text-center"
          >
            <span className="mark">◇</span>
            {/* `text-fail` klasą, nie `data-tone`: ton maluje `.lead` i `.value`, a to zdanie
                żadnej z tych ról nie nosi (2026-08-31). */}
            <p className="text-fail">{refusal}</p>
            <p className="lead max-w-160">
              Nothing is lost — every file is still on disk. A folder reads this way when Loadout
              has not been given access to it, which you grant under System Settings ▸ Privacy &amp;
              Security ▸ Files and Folders, or when the folder itself has been moved, renamed or
              removed.
            </p>
            {/* JEDYNA czynność główna na tym ekranie i jedyna, jaka ma tu sens: nic innego nie
                da się stąd zrobić, dopóki katalog nie odpowie. */}
            <button
              data-retry
              type="button"
              className="btn-primary"
              onClick={() => {
                void actions.load();
              }}
            >
              Try again
            </button>
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
          /* SIATKA MA POCZĄTEK, NIE ŚRODEK (`content-start`), i dwie kolumny, nie trzy: przy
             sześciu plikach trzecia kolumna zamienia karty w wizytówki i dokłada pustki, zamiast
             ją zabierać. Karta bohatera bierze obie kolumny. */
          <ul className="grid content-start gap-3 sm:grid-cols-2">
            {hero === null ? null : entryOnTheList(hero, true)}
            {rest.map((entry) => entryOnTheList(entry, false))}

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
            {/* `self-start` — POJEMNIK MA WYSOKOŚĆ TREŚCI, NIE WIERSZA. Bez tego kafelek
                tworzenia rozciągał się na wysokość najwyższej karty w swoim wierszu i był
                największym pustym prostokątem na ekranie (zmierzone na zrzucie 2026-08-31). */}
            <li className="flex flex-col gap-2 self-start">
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
