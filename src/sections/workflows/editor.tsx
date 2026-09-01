/* Edytor workflow: płótno plus panel kroku — ekran, po który ta sekcja w ogóle istnieje.
 *
 * DLACZEGO TEN PLIK POWSTAŁ. `canvas.tsx` (płótno), `step-panel/panel.tsx`, `skills-row.tsx`
 * i `checkpoint-panel.tsx` istnieją, mają testy i do 2026-08-17 **nie miały ani jednego miejsca
 * montowania**: sekcja Workflow renderowała wyłącznie listę, więc do edytora nie prowadziło
 * ani jedno kliknięcie. To ta sama rodzina, co zaślepki adapterów i `Limiter` z T-21 —
 * mechanizm wylądował, ma testy, nikt go nie podłączył. Test renderujący komponent wprost
 * nie odróżnia „zamontowane" od „istnieje".
 *
 * KTÓRE Z TYCH CZTERECH DOSTAŁY MONTAŻ I KIEDY — żeby następny czytelnik nie musiał tego
 * gerpować:
 *   2026-08-17: `canvas/canvas.tsx` (niżej, `WorkflowCanvas`) i `step-panel/panel.tsx`
 *               (`StepPanel`, montowany dziś przez `PanelForStep`).
 *   2026-08-18: `step-panel/checkpoint-panel.tsx` — przez `PanelForStep`, czyli rozjazd
 *               w `panel.tsx`. Do tego dnia ten plik miał w CAŁYM repo zero importerów, więc
 *               punkt kontrolny postawiony na płótnie nie miał jak dostać ani nazwy, ani
 *               pytania: bieg zatrzymywał się na nim i nie pytał o nic.
 *   2026-08-18, wieczór: `step-panel/skills-row.tsx` — ostatni z czterech. Jedynym jego
 *               importerem był jego własny test, a martwa była razem z nim akcja magazynu
 *               `chooseSkills`: mechanizm, test i zero wołających, trzy razy pod rząd.
 *
 * Przy okazji przestał obowiązywać warunek, który tę listę trzymał krótką: panel montował się
 * tylko wtedy, gdy `step.agent` rozwiązał się w bibliotece. `freshStep` daje `agent: ''`
 * z rozmysłem, więc krok prosto z przycisku nie rozwiązywał się NIGDY. Trzy odpowiedzi na
 * pytanie „jaki panel dostaje ten kafelek" mieszkają od teraz w jednym miejscu (`PanelForStep`).
 *
 * Magazyn dokumentu powstaje NA OTWARTY PLIK i ginie przy zamknięciu: `createWorkflowStore`
 * bierze dokument w konstruktorze, bo magazyn bez dokumentu nie ma sensu (`state/workflows.ts`).
 * Trzymanie jednego magazynu na całą sekcję wymagałoby `document: null`, czyli stanu, który
 * tamten plik świadomie wyklucza.
 *
 * NAGŁÓWEK JEST TERAZ NAGŁÓWKIEM Z MAKIETY (`.hd`, `docs/mockup/index.html:543-551`), i to jest
 * naprawa trzech osobnych wad naraz:
 *   `All workflows` zamiast nagiej strzałki „←" — strzałka bez podpisu nie mówi, gdzie prowadzi.
 *   NAZWA JAKO POLE, nie `<h1>`. Do 2026-08-18 nazwy nie dało się zmienić z okna wcale, więc na
 *   dysku właściciela leżały „New workflow" i „New workflow 2" — a to wprost napędzało następny
 *   defekt, bo Run brał plik pierwszy w sortowaniu bajtowym.
 *   `N steps · <stan zapisu>` — dowód, że zapis się odbył. Bez niego autosave jest niewidzialny
 *   w obie strony: człowiek nie wie ani że zapisał, ani (przy odmowie) że właśnie stracił pracę.
 */
import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';

import type { Agent } from '../../state/agents';
import type { Step, WorkflowFile } from '../../state/workflows';
import { createWorkflowStore } from '../../state/workflows';
import * as agentsIo from '../agents/io';
import { WorkflowCanvas } from './canvas/canvas';
import * as disk from './io';
import { PanelForStep } from './step-panel/panel';

const QUIET = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

export interface WorkflowEditorProps {
  /** Nazwa pliku, pod którą ten dokument leży na dysku. */
  path: string;
  /** Dokument wczytany przez sekcję — edytor go nie ładuje, tylko pokazuje i zmienia. */
  document: WorkflowFile;
  /**
   * Rewizja pliku Z CHWILI ODCZYTU, ta sama, którą oddał `Disk.load` razem z dokumentem.
   *
   * Jedzie prosto do magazynu i wraca z każdym zapisem: Rust odmawia publikacji, kiedy pod tą
   * nazwą leży co innego, więc bez tego pola okno otwarte pięć minut temu kasuje pracę
   * zapisaną minutę temu i wygląda przy tym na zapisujące poprawnie (2026-08-28).
   *
   * `null` znaczy „to okno nie widziało pliku" — tak wygląda tylko edytor postawiony w teście
   * wprost, bo produkcyjna droga zawsze przechodzi przez odczyt (`./index.tsx`).
   */
  revision?: string | null;
  /** Agenci z biblioteki: panel kroku pokazuje wartości efektywne, więc musi znać agenta. */
  agents: readonly Agent[];
  /** Umiejętności leżące w katalogach agentów — wiersz Skills wybiera z nich, a nie z niczego. */
  skills?: readonly string[];
  onClose: () => void;
  onRun: (path: string) => void;
  /** Skrót na sekcję Agents: „nie mam kogo wybrać" ma mieć drogę wyjścia, nie samą informację. */
  onCreateAgent: () => void;
  /**
   * Kafelek, którego panel jest otwarty już w chwili zamontowania ekranu.
   *
   * Bez propsu edytor otwiera panel dopiero na kliknięcie w kafelek — dokładnie tak, jak
   * powłoka bierze swoje ekrany bez propsu `screens` (`src/App.tsx`), a sekcja swój magazyn
   * bez propsu `store` (`./index.tsx`). Ten ekran ma stan, którego `renderToStaticMarkup`
   * nie umie ruszyć (nie ma kliknięcia i nie biegną efekty), więc bez tego wejścia jedyną
   * sprawdzalną odpowiedzią na pytanie „czy zaznaczenie daje panel" byłoby wyrenderowanie
   * panelu wprost — czyli asercja, która nie odróżnia „zamontowane" od „istnieje".
   */
  openStep?: string;
}

/** Dokument z podmienionym jednym krokiem. Podmiana idzie przez `commit`, czyli tę jedną
 * drogę, którą nowy dokument wchodzi do stanu (i pod którą wisi autosave). */
function withStep(file: WorkflowFile, id: string, change: (step: Step) => Step): WorkflowFile {
  return { ...file, steps: file.steps.map((step) => (step.id === id ? change(step) : step)) };
}

/** `1 step`, ale `4 steps`. Ta sama odmiana, co na kafelku listy (`list/tile.tsx`). */
function counted(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${String(count)} ${noun}s`;
}

/** Odmowa zapisu jako fakt widoczny dla człowieka, także gdy Rust oddał tylko techniczny powód. */
function visibleSaveRefusal(said: string): string {
  return said.toLowerCase().includes('not saved') ? said : `This workflow was not saved. ${said}`;
}

export function WorkflowEditor({
  path,
  document,
  revision = null,
  agents,
  skills = [],
  onClose,
  onRun,
  onCreateAgent,
  openStep,
}: WorkflowEditorProps): ReactElement {
  /* Magazyn powstaje DOKŁADNIE RAZ na zamontowanie tego ekranu — inicjalizator `useState`
   * biegnie tylko przy pierwszym renderze.
   *
   * Dokument jest tu ZIARNEM, nie wejściem: magazyn od tej chwili sam go trzyma i zmienia,
   * a przebudowa przy każdej edycji kasowałaby uwagi walidatora i odliczanie autosave'u.
   * Wymiana pliku odbywa się więc przez PRZEMONTOWANIE — sekcja podaje `key={path}` — a nie
   * przez tablicę zależności, której `react-hooks` musiałaby pilnować wbrew sobie. Pierwsza
   * wersja stała na `useMemo` z ręcznie przyciętą listą zależności i dwoma wyciszeniami reguły
   * hooków; odrzuciło je `quick-suppressions` i miało rację: wyciszenie ostrzeżenia jest tańsze
   * niż poprawny kształt tylko do chwili, w której ktoś dopisze tu drugie wejście. */
  const [store] = useState(() =>
    createWorkflowStore(
      {
        /* `write` bierze ścieżkę i dokument, `WorkflowIo.save` — dokument i rewizję. Ścieżka
         * jest domknięta tutaj, bo to edytor wie, który plik ma otwarty; magazyn tego nie wie
         * i nie powinien (drugie miejsce z odpowiedzią „gdzie to leży"). Rewizję niesie
         * magazyn, bo to ona zmienia się przy każdym zapisie. */
        save: (file, expectedRevision) => disk.write(path, file, expectedRevision),
        check: disk.check,
        /* Zapis AGENTA, nie kroku. Panel ma w liście „Save to the agent", a ta droga jest
         * jedyną, przez którą wolno jej dojechać do pliku agenta (`state/workflows.ts` §8).
         *
         * Rewizję pliku agenta czytamy TUTAJ, tuż przed zapisem, zamiast przewlekać ją przez
         * płótno i panele: nie jest ona faktem o otwartym workflow. Okno między odczytem
         * a publikacją zamyka Rust — to on porównuje bajty (`agents/io.ts`, `revisionOf`). */
        saveAgent: async (agent) => {
          await agentsIo.save(agent, await agentsIo.revisionOf(agent.id));
        },
      },
      document,
      revision,
    ),
  );

  const state = store();
  const [openStepId, setOpenStepId] = useState<string | null>(openStep ?? null);

  /* Uwagi walidatora bierzemy przy otwarciu, nie dopiero po pierwszej zmianie: workflow zapisany
   * wczoraj i zepsuty od wczoraj ma powiedzieć o tym od razu, a nie po dotknięciu kafelka.
   *
   * Czytamy przez `store.getState()`, a nie przez migawkę `state`: migawka zmienia się przy
   * każdej edycji, więc w zależnościach kazałaby temu efektowi biec po każdym naciśnięciu
   * klawisza. Magazyn jest stały przez całe życie tego ekranu, więc lista zależności jest
   * uczciwa i kompletna — bez wyciszania czegokolwiek. */
  useEffect(() => {
    void store.getState().recheck();
  }, [store]);

  /* KAŻDY kafelek, nie tylko krok agenta z rozwiązanym agentem. Rodzaj kroku i to, czy agent
   * jest już wybrany, rozstrzyga `PanelForStep` — tutaj zostaje jedno pytanie: który kafelek
   * jest zaznaczony. Warunek zawężający, który stał w tej linii do 2026-08-18, robił z połowy
   * kafelków kafelki bez panelu, a wyglądało to jak brak zaznaczenia. */
  const open = state.document.steps.find((step) => step.id === openStepId);

  /* Stan zapisu, policzony z JEDNEGO faktu: czy dokument na ekranie to ten sam obiekt, który
   * poszedł na dysk. Bez zegara i bez licznika — „saved just now" z makiety jest prawdą przez
   * kilka sekund, a potem jest zdaniem, które ekran powtarza w nieskończoność. */
  const saveState = state.document === state.savedDocument ? 'saved' : 'saving…';

  return (
    <section className="flex h-full min-h-0 flex-col">
      <header className="flex h-13 shrink-0 items-center gap-3 border-b border-line bg-panel px-4">
        <button type="button" className={QUIET} onClick={onClose}>
          All workflows
        </button>
        {/* Nazwa jako POLE. `aria-label`, a nie widoczna etykieta: wiersz nagłówka z makiety ma
            tu tytuł, nie formularz, a pole bez żadnej nazwy dostępnej jest polem, o którym
            czytnik ekranu nie umie nic powiedzieć. */}
        <input
          id="workflow-name"
          aria-label="Workflow name"
          className="min-w-0 flex-1 rounded-sm border border-transparent bg-transparent px-1 text-title text-ink hover:border-line focus:border-line-strong"
          value={state.document.name}
          onChange={(event) => {
            state.rename(event.target.value);
          }}
        />
        <span className="shrink-0 font-mono text-mono text-muted">
          {counted(state.document.steps.length, 'step')} · {saveState}
        </span>
        {/* Uwagi walidatora są faktem o dokumencie i mieszkają w jednym miejscu — tutaj.
         * Płótno ich nie liczy i nie tłumaczy (`canvas.tsx`).
         *
         * „things to fix", NIE „problems", i to nie jest zmiana kosmetyczna. Ta lista niesie dwie
         * wagi: `problem` blokuje Run, `warning` nie blokuje niczego (`canvas/problems.tsx`).
         * Odkąd kafelek dołożony luzem jest normalnym stanem pracy, ostrzeżenia są tu regułą,
         * a nie wyjątkiem — a plakietka mówiąca „2 problems" nad szkicem, który zapisuje się
         * i uruchamia bez przeszkód, nazywa problemem coś, co nim nie jest. Brzmienie jest
         * dokładnie to samo, co na pasku nad przyciskiem Run: jeden fakt, jedno słowo. */}
        {state.notes.length === 0 ? null : (
          <span className="shrink-0 rounded-pill border border-attend-edge bg-attend-soft px-2 font-mono text-label text-attend">
            {state.notes.length === 1
              ? '1 thing to fix'
              : `${String(state.notes.length)} things to fix`}
          </span>
        )}
      </header>

      {/* PASEK ODMOWY ZAPISU. Nie ma go w makiecie i to jest świadome: makieta nie przewiduje
          stanu „plik na dysku nie jest tym, co widzisz", bo powstała przed pomiarem, który ten
          stan wykrył. Kontrolki tu nie ma żadnej — zdanie znika samo, kiedy następny zapis się
          uda (`saveNow` czyści pole), więc „OK" byłoby przyciskiem, który kasuje wiadomość
          o nadal niezapisanym pliku. */}
      {state.couldNotSave === null ? null : (
        <p
          data-could-not-save
          className="shrink-0 border-b border-fail-edge bg-fail-soft px-4 py-2 text-body text-fail"
        >
          {visibleSaveRefusal(state.couldNotSave)}
        </p>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_330px]">
        <div className="min-h-0 overflow-auto p-3.5">
          <WorkflowCanvas
            document={state.document}
            agents={agents}
            notes={state.notes}
            onChange={state.commit}
            onRun={() => {
              /* 2026-08-28 (T-151): Run dostaje wyłącznie nazwę pliku, więc jedynym sposobem
               * zagwarantowania widocznej rewizji jest poczekać, aż store potwierdzi co najmniej
               * tę rewizję. `saveNow` stoi w tym samym szeregowym ogonie co autosave; odmowa
               * zostawia ten ekran zamontowany i sama wystawia zdanie powyżej. */
              void store
                .getState()
                .saveNow()
                .then(() => {
                  onRun(path);
                })
                .catch(() => undefined);
            }}
            onOpenPanel={setOpenStepId}
            /* AUTO-FIX. Biblioteka jedzie argumentem, tą samą drogą co przy `editStep`:
               magazyn dokumentu jej nie trzyma i trzymać nie powinien. */
            onApplyFix={(fix) => {
              void state.applyFix(fix, agents);
            }}
          />
        </div>

        {/* Panel kroku, szerokość 330 px prosto z makiety (`.side`), padding 14 px = `p-3.5`.
            RAMKA JEST TU I TYLKO TU: do 2026-08-18 każdy z trzech paneli rysował własne
            `<aside>` o szerokości 328 px wewnątrz tego, więc pola były ucięte, a panel miał
            własny poziomy pasek przewijania. Bez otwartego kroku kolumna zostaje pusta, zamiast
            znikać: znikająca kolumna przesuwa płótno pod kursorem w chwili kliknięcia. */}
        <aside className="min-h-0 overflow-auto border-l border-line bg-panel p-3.5">
          {open === undefined ? (
            <p className="text-muted">Pick a step to see what it was given.</p>
          ) : (
            <PanelForStep
              /* KLUCZ NA IDENTYFIKATORZE KROKU. Panel niesie stan WIDOKU (rozwinięta lista
                 wyboru agenta i wpisane w niej szukanie), a bez klucza React trzymałby ten sam
                 egzemplarz przy przeskoku na inny kafelek: zaznaczenie świeżego kroku po kroku
                 z agentem dostawałoby listę zwiniętą, czyli inny panel niż ten sam krok
                 zaznaczony jako pierwszy. */
              key={open.id}
              step={open}
              agents={agents}
              skills={skills}
              onCreateAgent={onCreateAgent}
              onChooseAgent={(agentId) => {
                /* Wybór agenta jest polem KROKU, nie nadpisaniem agenta, więc jedzie tą samą
                 * drogą co nazwa — `commit` — a nie przez `editStep`, które liczy różnicę
                 * wobec agenta i bez agenta nie miałoby czego odjąć. */
                state.commit(
                  withStep(state.document, open.id, (step) =>
                    /* Rodzaj sprawdzamy jeszcze raz, bo `withStep` widzi krok z dokumentu,
                     * a nie ten zawężony przez panel: czytanie z dokumentu, nie z domknięcia,
                     * jest tym, co trzyma dwie zmiany pod rząd na tym samym stanie. */
                    step.kind === 'agent' ? { ...step, agent: agentId } : step,
                  ),
                );
              }}
              onEdit={(agent, edit) => {
                /* Agent jedzie ARGUMENTEM, bo panel podaje wartości EFEKTYWNE, a różnicę
                 * wobec agenta liczy magazyn (`applyPanelEdit`). Bez tego edytor musiałby
                 * wiedzieć, co jest nadpisaniem, a co dziedziczeniem — czyli drugi raz. */
                state.editStep(open.id, agent, edit);
              }}
              onEditStep={(fields) => {
                /* Pola własne kroku (nazwa, instrukcje, `copies`) nie są nadpisaniami agenta,
                 * więc nie mają osobnej akcji: jadą przez `commit`, czyli tę jedną drogę,
                 * którą nowy dokument wchodzi do stanu (i pod którą wisi autosave). */
                state.commit(
                  withStep(state.document, open.id, (step) =>
                    step.kind === 'agent' ? { ...step, ...fields } : step,
                  ),
                );
              }}
              /* Powrót WYCHODZĄCY z tego kroku, jeżeli jakiś jest. Liczony tutaj, nie w panelu:
                 panel dostaje jedną liczbę i nie zna strzałek, więc nie ma jak pomylić się co do
                 tego, która z nich jest powrotem (niezmiennik 13). */
              wayBack={
                state.document.links.find(
                  (link) => link.from === open.id && link.max_turns !== undefined,
                )?.max_turns ?? null
              }
              onEditWayBack={(turns) => {
                /* Ta sama droga, co każda inna zmiana dokumentu: `commit`, pod którym wisi
                   autosave. Przepisujemy WYŁĄCZNIE powrót wychodzący z otwartego kroku — reszta
                   strzałek zostaje tymi samymi obiektami, więc porównanie referencji w Reakcie
                   dalej mówi prawdę o tym, co się zmieniło. */
                state.commit({
                  ...state.document,
                  links: state.document.links.map((link) =>
                    link.from === open.id && link.max_turns !== undefined
                      ? { ...link, max_turns: turns }
                      : link,
                  ),
                });
              }}
              onEditServe={(fields) => {
                /* Ta sama droga dla kafelka „uruchom i zostaw". Jego `command` jest jedynym
                 * powodem, dla którego ten kafelek w ogóle coś podnosi — pole, które nie dojeżdża
                 * do pliku, daje kafelek odmawiający w środku biegu. */
                state.commit(
                  withStep(state.document, open.id, (step) =>
                    step.kind === 'serve' ? { ...step, ...fields } : step,
                  ),
                );
              }}
              onEditCheck={(fields) => {
                /* Ta sama droga dla kafelka „sprawdź". Jego dwa pola tekstowe są całą treścią
                 * tego kroku, a wzorzec do tego ODMOWĄ ZAPISU po stronie Rusta, dopóki jest
                 * pusty (`check::a_command_step_left_empty`): pole, które nie dojeżdża do pliku,
                 * daje workflow, który przestaje się zapisywać, i człowieka szukającego przyczyny
                 * w kafelku, który na ekranie wygląda na wypełniony. */
                state.commit(
                  withStep(state.document, open.id, (step) =>
                    step.kind === 'check' ? { ...step, ...fields } : step,
                  ),
                );
              }}
              onEditCheckpoint={(fields) => {
                /* Ta sama droga dla punktu kontrolnego. Jego `question` jest jedynym powodem,
                 * dla którego bieg ma się na nim o cokolwiek zapytać — pole, które nie dojeżdża
                 * do pliku, daje bieg stojący w miejscu i milczący. */
                state.commit(
                  withStep(state.document, open.id, (step) =>
                    step.kind === 'checkpoint' ? { ...step, ...fields } : step,
                  ),
                );
              }}
              onReset={(field) => {
                state.resetRow(open.id, field);
              }}
              onChooseSkills={(choice) => {
                /* Akcja magazynu, która do 2026-08-18 nie miała ANI JEDNEGO wołającego. */
                state.chooseSkills(open.id, choice);
              }}
            />
          )}
        </aside>
      </div>
    </section>
  );
}
