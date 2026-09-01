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
import {
  askTheStepBefore,
  fieldWaitedFor,
  handsOver,
  theStepBefore,
} from './step-panel/hands-over-the-command';
import { useEffect, useState, useSyncExternalStore } from 'react';

import type { Agent } from '../../state/agents';
import type { Step, WorkflowFile } from '../../state/workflows';
import { createWorkflowStore } from '../../state/workflows';
import * as agentsIo from '../agents/io';
import { WorkflowCanvas } from './canvas/canvas';
import type { NoteFocus } from './canvas/problems';
import { RunButton, ThingsToFix, focusNote, howMany } from './canvas/problems';
import * as disk from './io';
import { PanelForStep } from './step-panel/panel';

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

  /* Magazyn czytamy przez `useSyncExternalStore` ze STANEM BIEŻĄCYM w trzecim argumencie,
   * a nie hakiem `store()` — 2026-08-31, ta sama poprawka i ten sam powód, co w `./index.tsx`
   * i w `../agents/index.tsx`.
   *
   * Zustand 5 podaje rendererowi serwerowemu `getInitialState()` jako migawkę serwerową
   * (`node_modules/zustand/esm/react.mjs`), więc pod `renderToStaticMarkup` ten ekran pokazywał
   * stan Z CHWILI UTWORZENIA magazynu i nigdy tego, co się w nim potem wydarzyło. W oknie nie
   * zmienia to ANI JEDNEJ klatki — ta aplikacja nigdy nie hydratuje serwerowego HTML-a, więc
   * powód, dla którego React chce tam stanu początkowego, tutaj nie istnieje. Zmienia natomiast
   * to, czy da się w ogóle napisać kryterium na zdanie, które pojawia się PO odmowie dysku:
   * bez tego pasek `data-could-not-save` i stan zapisu w nagłówku były niesprawdzalne w całym
   * repo (jsdomu tu nie ma), a niesprawdzalne zdanie to zdanie, które umiera po cichu. */
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const [openStepId, setOpenStepId] = useState<string | null>(openStep ?? null);

  /* Czy lista uwag jest rozwinięta. Zwinięta na starcie i przy każdym wejściu w plik — powód
   * w całości stoi przy plakietce w nagłówku. */
  const [showNotes, setShowNotes] = useState(false);

  /* Prośba do płótna: „przesuń się na ten krok". NOWY OBIEKT JEST SYGNAŁEM, więc dwa kliknięcia
   * w tę samą uwagę przesuwają płótno dwa razy; `null` znaczy „nikt o nic nie prosił".
   *
   * Ekran nie umie tego zrobić sam i to nie jest przeoczenie: `fitView` przychodzi z hooka
   * `useReactFlow()`, czyli działa wyłącznie WEWNĄTRZ `ReactFlowProvider`, a ten stoi w płótnie.
   * Uwagi wyprowadziliśmy 2026-08-31 do nagłówka, więc kliknięcie w uwagę dzieje się na zewnątrz
   * tamtego drzewa i musi dojechać propsem. Argumenty składa `focusNote` — czas i sufit
   * powiększenia są faktem o uwadze i mieszkają razem z nią (`canvas/problems.tsx`). */
  const [bringIntoView, setBringIntoView] = useState<{
    view: Parameters<NoteFocus['fitView']>[0];
  } | null>(null);

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

  /* Stan zapisu — TRZY wartości, nie dwie, i to jest naprawa z 2026-08-31 (zgłoszenie
   * właściciela).
   *
   * Pierwsza połowa stoi tu od początku i jest w porządku: „czy dokument na ekranie to ten sam
   * obiekt, który poszedł na dysk". Bez zegara i bez licznika — „saved just now" z makiety jest
   * prawdą przez kilka sekund, a potem jest zdaniem, które ekran powtarza w nieskończoność.
   *
   * BRAKOWAŁO TRZECIEJ. Po odmowie zapisu `savedDocument` się NIE ZMIENIA — ustawia je wyłącznie
   * gałąź sukcesu w `state/workflows.ts` — więc to samo porównanie mówiło „saving…" już na
   * zawsze, choć nic się nie zapisywało i nie miało zapisać. Czerwony pasek niżej mówił prawdę,
   * a nagłówek nad nim kłamał: dwa zdania o jednym fakcie, sprzeczne (niezmiennik 13).
   *
   * Podział pracy między tym napisem a paskiem jest ostry i celowy: nagłówek nazywa STAN
   * („not saved"), pasek nazywa POWÓD („…changed on disk…"). Sam stan kazałby szukać przyczyny,
   * sam powód nie mówi, czy coś jeszcze trwa. */
  const saveState =
    state.document === state.savedDocument
      ? 'saved'
      : state.couldNotSave === null
        ? 'saving…'
        : 'not saved';

  return (
    <section className="flex h-full min-h-0 flex-col">
      {/* `relative`, bo pasek trwania zapisu wisi na dolnej krawędzi tego paska i nie ma prawa
          przesunąć niczego w układzie: przy autozapisie co 400 ms wiersz, który raz jest, a raz
          go nie ma, przeskakiwałby treścią pod spodem przy każdym naciśnięciu klawisza.

          `.screen-head` NIE MA TŁA z rozmysłu (theme.css): jest chrome, więc materiał bierze
          z klasy materiału obok. */}
      {/* `z-20` od 2026-08-31: lista uwag wychodzi spod plakietki NAD ciało ekranu, a ciało
          jest w drzewie PÓŹNIEJ i też jest `relative`, więc bez warstwy malowałoby się na
          wierzchu i lista chowałaby się pod płótnem. */}
      <header className="screen-head glass relative z-20">
        {/* WYJŚCIE JEST CICHE — `btn-bare`, nie `btn-quiet`, od 2026-08-31 (zgłoszenie
            właściciela). Nie dlatego, że przestało być potrzebne, tylko dlatego, że głośność
            w tym wierszu jest faktem WZGLĘDNYM: obrys wokół drogi powrotnej stojący obok nazwy
            bez obrysu robił z nawigacji najgłośniejszą rzecz w nagłówku, a z tytułu dokumentu
            najcichszą z trzech. Napis zostaje ten sam — strzałka bez podpisu nie mówi, dokąd
            prowadzi (powód z 2026-08-18, dalej w mocy). */}
        <button type="button" className="btn-bare" onClick={onClose}>
          All workflows
        </button>
        {/* Nazwa jako POLE. `aria-label`, a nie widoczna etykieta: wiersz nagłówka z makiety ma
            tu tytuł, nie formularz, a pole bez żadnej nazwy dostępnej jest polem, o którym
            czytnik ekranu nie umie nic powiedzieć.

            OBRYS PRZY SPOCZYNKU, 2026-08-31. Do dziś stało tu `border-transparent`, czyli pole
            widoczne dopiero PO najechaniu — a to jest pole, którego nikt nie znajdzie, jeśli
            akurat nie przesunie nad nie kursora. Skutek był mierzalny i już raz opisany dwa
            akapity wyżej: na dysku właściciela leżały „New workflow" i „New workflow 2", bo
            nazwy nie dało się zmienić; od 2026-08-18 dało się, tylko nie było tego widać.
            Trzy stopnie zamiast dwóch — `line` przy spoczynku, `line-strong` pod kursorem
            i przy skupieniu — bo obrys, który przy najechaniu nie robi nic, znowu nie mówi,
            że to jest kontrolka. */}
        <input
          id="workflow-name"
          aria-label="Workflow name"
          className="min-w-0 flex-1 rounded-sm border border-line bg-transparent px-2 text-title text-ink hover:border-line-strong focus:border-line-strong"
          value={state.document.name}
          onChange={(event) => {
            state.rename(event.target.value);
          }}
        />
        {/* `data-tone="fail"` TYLKO w trzecim stanie: barwa nasycona znaczy w tej aplikacji
            „zepsute", a zapis w toku ani plik zgodny z ekranem zepsute nie są (DESIGN §3). */}
        <span className="value shrink-0" data-tone={saveState === 'not saved' ? 'fail' : undefined}>
          {counted(state.document.steps.length, 'step')} · {saveState}
        </span>
        {/* Uwagi walidatora są faktem o dokumencie i mieszkają w jednym miejscu — tutaj.
         * Płótno ich nie liczy i nie tłumaczy (`canvas.tsx`).
         *
         * „things to fix", NIE „problems", i to nie jest zmiana kosmetyczna. Ta lista niesie dwie
         * wagi: `problem` blokuje Run, `warning` nie blokuje niczego (`canvas/problems.tsx`).
         * Odkąd kafelek dołożony luzem jest normalnym stanem pracy, ostrzeżenia są tu regułą,
         * a nie wyjątkiem — a plakietka mówiąca „2 problems" nad szkicem, który zapisuje się
         * i uruchamia bez przeszkód, nazywa problemem coś, co nim nie jest.
         *
         * PLAKIETKA JEST OD 2026-08-31 KONTROLKĄ, a liczbę pisze `howMany` z `problems.tsx`.
         * Dwie zmiany, jeden powód (niezmiennik 13). Do dziś to samo zdanie powstawało DWA RAZY:
         * tutaj, wpisane w JSX, i drugi raz w `RunBar` nad przyciskiem Run w rogu płótna —
         * dwa kawałki kodu liczące jeden fakt, więc pierwsza zmiana brzmienia rozjechałaby je
         * po cichu. Zdanie mieszka teraz tam, gdzie podział na `problem` i `warning`, a ekran
         * je WOŁA.
         *
         * DLACZEGO ZOSTAJE PLAKIETKA, A LISTA WYCHODZI SPOD NIEJ — a nie odwrotnie. Uwagi są
         * dziś REGUŁĄ, nie wyjątkiem (kafelek dołożony luzem zgłasza się sam), a jedna reguła
         * walidatora zgłasza się PER PARĘ kroków, więc dziesięć nienazwanych kafelków daje
         * czterdzieści pięć zdań. Lista rozwinięta na stałe zajmowałaby ekran tym częściej,
         * im mniej gotowy jest szkic — i to właśnie robiła: przy trzech uwagach spychała Run
         * o ~150 px, czyli główna akcja wędrowała tym bardziej, im więcej było powodów, żeby
         * na nią patrzeć. Plakietka mówi JEDNĄ liczbę i nie rusza się nigdy; zdania są jedno
         * kliknięcie dalej i wychodzą NAD ekran, więc nie przesuwają niczego. */}
        {state.notes.length === 0 ? null : (
          <button
            type="button"
            data-things-to-fix
            className="chip shrink-0"
            data-tone="attend"
            aria-expanded={showNotes}
            onClick={() => {
              setShowNotes(!showNotes);
            }}
          >
            {howMany(state.notes)}
          </button>
        )}
        {/* GŁÓWNA AKCJA TEGO EKRANU, i od 2026-08-31 wygląda na główną (zgłoszenie właściciela).
            Do tego dnia `Run` był małą nakładką w rogu płótna, a `Tidy up` — czynność porządkowa
            używana raz na dziesięć razy — miał pełnowymiarowy przycisk w rzędzie na dole.
            Odwrócona waga.

            Nie jest to nowy wzorzec: `.btn-primary` na końcu `.screen-head` jest w tym produkcie
            miejscem głównej akcji KAŻDEGO ekranu (`＋ Create` na liście workflow, `list/workflow-
            list.tsx`), a makieta rysowała `Run` w nagłówku od początku. Sam przycisk dalej
            mieszka w `problems.tsx`, bo odpowiedź „czy da się uruchomić" jest tą samą listą uwag,
            co plakietka obok — polityka zostaje w rdzeniu, ekran ją tylko montuje (niezmiennik 23). */}
        <RunButton
          notes={state.notes}
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
        />
        {/* CZY TO TRWA — druga z trzech rzeczy, na które ruch ma prawo odpowiadać (DESIGN §7).
            Zapis na dysk jest tu jedyną operacją, która trwa dłużej niż jedna klatka, a do
            2026-08-31 mówiło o niej wyłącznie słowo `saving…`: napis, który zmienia się
            w miejscu, czyta się jak stan, a nie jak czynność. Pasek jest NIEOKREŚLONY, bo
            zapisu przez granicę IPC nikt nie liczy w procentach. */}
        {/* 2026-08-31 — WARUNEK JEST NA `saving…`, nie na „cokolwiek innego niż saved". Ten pasek
            odpowiada na pytanie „czy to trwa", a po odmowie dysku nie trwa NIC: chodzący w kółko
            pasek nad zdaniem „not saved" każe czekać na coś, co się nie wydarzy. */}
        {saveState !== 'saving…' ? null : (
          <span aria-hidden className="working pointer-events-none absolute inset-x-0 bottom-0" />
        )}

        {/* ZDANIA O UWAGACH, jedno kliknięcie od plakietki, która je liczy. Wychodzą NAD ekran
            (`absolute`), więc nie spychają ani płótna, ani przycisku Run — a to była cała wada
            starego miejsca: przy trzech uwagach lista przesuwała Run o ~150 px w dół.

            Kotwiczy się do nagłówka, nie do ekranu: `.screen-head` jest `relative`, a `right-4`
            to ten sam padding 16 px, który nagłówek ma po bokach — więc lista kończy się równo
            z prawą krawędzią przycisku Run, spod którego wychodzi. */}
        {showNotes && state.notes.length > 0 ? (
          <div
            data-things-to-fix-list
            className="card enter absolute top-full right-4 mt-1 max-h-96 w-96 overflow-auto shadow-md"
          >
            <ThingsToFix
              notes={state.notes}
              onFocusNote={(note) => {
                /* Kliknięcie uwagi robi DWIE rzeczy i obie są tą samą odpowiedzią na „gdzie to
                 * jest": płótno przesuwa się na winny krok, a jego panel się otwiera. Rozdział
                 * ról jest wymuszony — pierwsze umie tylko płótno (`fitView` z `useReactFlow`),
                 * drugie tylko ten ekran (to on trzyma zaznaczenie).
                 *
                 * Lista zwija się w `openPanel`, a nie tutaj, i to nie jest przypadek: `focusNote`
                 * woła `openPanel` WYŁĄCZNIE dla uwagi, która nazywa krok. Uwaga o całym pliku
                 * („There are no steps yet.") nie ma w co celować, więc zwinięcie listy byłoby
                 * jedynym skutkiem kliknięcia — czyli kliknięciem, które sprząta ekran zamiast
                 * odpowiedzieć. Warunek stoi w jednym miejscu, w `focusNote`, i nie ma tu kopii. */
                focusNote(note, {
                  fitView: (view) => {
                    setBringIntoView({ view });
                  },
                  openPanel: (stepId) => {
                    setOpenStepId(stepId);
                    setShowNotes(false);
                  },
                });
              }}
              /* AUTO-FIX. Biblioteka jedzie argumentem, tą samą drogą co przy `editStep`:
                 magazyn dokumentu jej nie trzyma i trzymać nie powinien. */
              onApplyFix={(fix) => {
                void state.applyFix(fix, agents);
              }}
            />
          </div>
        ) : null}
      </header>

      {/* PASEK ODMOWY ZAPISU. Nie ma go w makiecie i to jest świadome: makieta nie przewiduje
          stanu „plik na dysku nie jest tym, co widzisz", bo powstała przed pomiarem, który ten
          stan wykrył. Kontrolki tu nie ma żadnej — zdanie znika samo, kiedy następny zapis się
          uda (`saveNow` czyści pole), więc „OK" byłoby przyciskiem, który kasuje wiadomość
          o nadal niezapisanym pliku. */}
      {state.couldNotSave === null ? null : (
        <p
          data-could-not-save
          /* Wejście na przezroczystości, nie sprężyną: to jest zdanie do przeczytania, a nie
             powierzchnia, która wjeżdża. Pasek błędu zostaje paskiem błędu z DESIGN §6 —
             `border-b`, wypełnienie `-soft`, żadnego promienia. */
          className="fade-in shrink-0 border-b border-fail-edge bg-fail-soft px-4 py-2 text-body text-fail"
        >
          {visibleSaveRefusal(state.couldNotSave)}
        </p>
      )}

      {/* PŁÓTNO I PANEL LEŻĄ NA SOBIE, NIE OBOK SIEBIE — 2026-08-31, zgłoszenie właściciela ze
          zrzutu z okna 1512 px.

          BYŁO: `grid-cols-[minmax(0,1fr)_330px]`, czyli kolumna panelu STAŁA. Bez zaznaczonego
          kroku rysowała jedno zdanie i 300 px pustki pod nim, a płótno dostawało 974 px z 1304
          dostępnych. Piąta część okna szła na powierzchnię, na której nic nie stało.

          DLACZEGO NIE DA SIĘ TEGO NAPRAWIĆ SAMYM ZNIKANIEM KOLUMNY. Komentarz, który tej pustki
          bronił, mówił prawdę i dalej ją mówi: kolumna, która znika po kliknięciu w kafelek,
          przesuwa płótno POD KURSOREM dokładnie w chwili, w której człowiek w nie celuje. Oba
          warunki muszą więc być spełnione naraz — pustka znika, a płótno nie drga.

          WYBRANA ODPOWIEDŹ: panel jest NAKŁADKĄ. `absolute` wewnątrz `relative`, czyli poza
          układem — pudełko płótna jest tym samym elementem o tych samych klasach z zaznaczonym
          krokiem i bez niego, więc szerokość płótna jest STAŁA z konstrukcji, a nie z ostrożności.
          Nie ma tu czego przeliczać po zmianie rozmiaru, bo zmiany rozmiaru nie ma.

          DWIE ODRZUCONE, obie z mierzalnego powodu, nie z gustu:
            zwijanie kolumny do paska z uchwytem ZMIENIA szerokość — mniej niż 330 px i tylko raz,
            ale warunek brzmi „nie drgnie", a nie „drgnie mniej";
            `fitView` przeliczany po zmianie szerokości przesuwa TREŚĆ, żeby zrekompensować ruch
            ramki: goni ruch zamiast go nie robić, a przy 400 ms animacji dokłada drugi ruch tam,
            gdzie miał być zero.

          CENĄ jest 330 px płótna ZASŁONIĘTE, kiedy panel stoi otwarty. To jest wymiana świadoma:
          zasłonięte wraca w całości po zamknięciu panelu i daje się odsunąć przeciągnięciem
          płótna, a stracone 330 px kolumny nie wracało nigdy i nie dawało się z niczym zrobić. */}
      <div data-canvas-body className="relative min-h-0 flex-1">
        <div data-canvas-area className="h-full min-h-0 overflow-auto p-3.5">
          {/* Płótno nie zna dziś uwag i nie zna przycisku Run — 2026-08-31. Oba jechały tu
              propsami wyłącznie po to, żeby zasilić nakładkę w rogu; oba stoją teraz w nagłówku
              ekranu, nad tym płótnem. Została jedna nitka w drugą stronę: `bringIntoView`. */}
          <WorkflowCanvas
            document={state.document}
            agents={agents}
            onChange={state.commit}
            onOpenPanel={setOpenStepId}
            bringIntoView={bringIntoView}
          />
        </div>

        {/* ZDANIE NA MIEJSCU PANELU, kiedy nic nie jest zaznaczone. Nie zajmuje układu
            (`pointer-events-none` plus `absolute`), więc kliknięcie przez nie trafia w płótno,
            a jego 330 px to dalej płótno — nie powierzchnia trzymana dla panelu.

            STOI DOKŁADNIE TAM, GDZIE WYJDZIE PANEL, i to jest cała jego druga robota: mówi nie
            tylko PO CO klikać w kafelek, ale też GDZIE się wtedy popatrzeć. Wyrzucone do rogu
            byłoby zdaniem o czymś, co pojawi się gdzie indziej.

            BRZMIENIE, 2026-08-31. Do dziś stało tu „Pick a step to see what it was given." —
            obietnica PODGLĄDU. Ta powierzchnia jest EDYTOREM: ustawia się w niej, co krok ma
            robić (agent, instrukcje, folder, komenda, pytanie punktu kontrolnego). Zdanie, które
            obiecuje oglądanie, opisywało tamtą kolumnę z czasów, kiedy panel naprawdę tylko
            pokazywał wartości efektywne. */}
        {open === undefined ? (
          <p className="lead pointer-events-none absolute inset-y-0 right-0 flex w-[330px] items-center justify-center p-3.5 text-center">
            Pick a step to set up what it does.
          </p>
        ) : (
          /* Panel kroku, szerokość 330 px prosto z makiety (`.side`), padding 14 px = `p-3.5`.
             RAMKA JEST TU I TYLKO TU: do 2026-08-18 każdy z trzech paneli rysował własne
             `<aside>` o szerokości 328 px wewnątrz tego, więc pola były ucięte, a panel miał
             własny poziomy pasek przewijania.

             `.enter` i cień: ta powierzchnia PŁYWA nad płótnem, więc mówi to jednym i drugim.
             `--shadow-md` jest w theme.css dokładnie na to („rzecz, ktora PLYWA") i bez niego
             panel czyta się jak wycięty kawałek płótna, a nie jak coś, co nad nim leży. */
          <aside
            data-step-editor
            className="enter absolute inset-y-0 right-0 w-[330px] overflow-auto border-l border-line bg-panel p-3.5 shadow-md"
          >
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
              /* KTO ODDAJE KOMENDĘ TEMU KAFELKOWI — liczone TUTAJ, bo tylko edytor zna strzałki.
                 Panel dostaje trzy gotowe odpowiedzi i nie ma jak pomylić się co do tego, który
                 krok jest tym przed (ten sam ruch, co przy `wayBack` wyżej). */
              stepBefore={theStepBefore(state.document, open.id)?.name ?? null}
              handsItOver={handsOver(
                theStepBefore(state.document, open.id),
                fieldWaitedFor(state.document, open.id),
              )}
              onAskTheStepBefore={() => {
                /* JAWNA ZMIANA CUDZEGO KAFELKA, na kliknięcie i tylko na nie. Ta sama droga
                   `commit`, co każda inna edycja — więc wchodzi do cofania i do autozapisu jak
                   wszystko inne, zamiast być osobnym trybem zapisu. */
                state.commit(
                  askTheStepBefore(
                    state.document,
                    open.id,
                    fieldWaitedFor(state.document, open.id),
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
          </aside>
        )}
      </div>
    </section>
  );
}
