/* Ekran sekcji `run`. Powłoka znajduje go po ŚCIEŻCE — `src/sections/<id>/index.tsx` — więc
 * ten plik jest całym wpisem do rejestru i nie ma żadnego drugiego miejsca, w którym trzeba by
 * go zadeklarować (T-25, HARNESS-QUEUE.md Q-5).
 *
 * CIENKI Z PREMEDYTACJĄ, I TO JEST CAŁY JEGO MANDAT: układ z makiety plus MIEJSCA MONTOWANIA.
 * Druga implementacja czegokolwiek z `feed/`, `strip/`, `tabs/`, `limits/` albo `rail/` byłaby
 * drugim miejscem prawdy o tej samej rzeczy (niezmiennik 23). Każdy blok niżej istniał już
 * w repo, miał własne testy i ani jednego wołającego spoza nich — to jest ta sama rodzina, co
 * płótno przed T-26 i `io.ts` przed T-38, i dokładnie ten powód, dla którego świeży ekran Run
 * miał 2026-08-18 jeden przycisk, i to wyłączony.
 *
 * UKŁAD JEST LUSTREM MAKIETY (`docs/mockup/index.html`, `data-screen="work"`): trzy rzędy —
 * karty, pasek loadoutu, praca — a praca to dwie kolumny: strumień i lista agentów o szerokości
 * `RAIL_WIDTH`. Deklaracje siatki stoją niżej jako stałe, żeby test mógł je porównać z regułami
 * `.work` i `.feedcol` przeczytanymi z makiety w tym samym biegu (T-37 AC-1 zrobił to samo
 * z regułą `.app`).
 *
 * NAGŁÓWEK SEKCJI zostaje mimo makiety, która na ekranie pracy nie ma żadnego. Wymaga go
 * `e2e/tests/sections-mount.spec.ts`: każdy ekran ma się nazwać własnym nagłówkiem, inaczej
 * „coś się zamontowało" nie odpowiada na pytanie, na której sekcji stoisz. Koszt jest zapisany
 * i nadal nie ma z czego zapłacony: ARCHITECTURE §7 daje 96 px nad pierwszą treścią, karty
 * biorą 34, pasek loadoutu 56, a ten pasek 52. Domknięcie należy do zadania, które posiada
 * pasek loadoutu — albo nazwa sekcji wchodzi W ten pasek, albo któryś z dwóch znika.
 *
 * SKĄD BIERZE SIĘ TREŚĆ. Z dwóch źródeł i każde odpowiada na inne pytanie. Model widoku
 * (`feed/live.ts`) trzyma wiersze historii, strefę TERAZ i przypięte pytanie; magazyn
 * (`state/run.ts`) trzyma okno linii i plan biegu, z którego rysuje się pasek. Oba są na
 * poziomie modułu, bo bieg trwa dłużej niż ten ekran: wyjście do Agentów odmontowuje komponent
 * i nie ma prawa skasować biegu.
 *
 * DLACZEGO MAGAZYNY CZYTAMY PRZEZ `useSyncExternalStore` Z BIEŻĄCYM STANEM JAKO MIGAWKĄ
 * SERWEROWĄ, a nie hakiem `useRun(selector)`. `renderToStaticMarkup` jest rendererem
 * serwerowym, a zustand 5 podaje mu `getInitialState`, więc ekran czytany hakiem pokazywałby
 * stan Z CHWILI UTWORZENIA magazynu i nigdy tego, co do niego weszło. Ta aplikacja nigdy nie
 * hydratuje serwerowego HTML-a, więc powód, dla którego React chce tam stanu początkowego,
 * tutaj nie istnieje. Ten sam zapis stoi w `src/sections/workflows/index.tsx`.
 */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import type { MouseEvent, ReactElement } from 'react';

import { why } from '../../ipc/why';
import { sectionEntry } from '../../ui/sections';
import type { FeedLine, Step } from '../../state/run';
import { useRun } from '../../state/run';
import { useWorkspaces } from '../../state/workspaces';
import { addresseeOf } from './addressee';
import type { WindowLine } from './entry/echo';
import { IMAGE_SEND_FAILED, IMAGES_TO_LEAD_ONLY } from './entry/images';
import type { ConversationImage } from './entry/images';
import { Feed } from './feed/feed';
import { attachPort, feedFor, runFeed } from './feed/live';
import type { FeedView } from './feed/model';
import { Now } from './feed/now';
import { Entry } from './entry/entry';
import { Diagnostics } from './diagnostics';
import { chooseWorkingFolder, folderName } from './folders';
import { openChat, sayToAgent, sayToOrchestrator, stop } from './io';
/* KIM JEST LIDER — jedno źródło, to samo, z którego czyta kontrolka w pasku (`./start.tsx`).
 * Ten ekran wskazania nie kopiuje i nie trzyma: pyta o nie w chwili wysyłki zdania. */
import { lead } from './lead';
import { atOnce as atOnceNow, subscribeToAtOnce } from './limits/chosen';
import { waitingWhere } from './limits/waiting';
import { toChoices } from './choices';
import type { Named } from './run-command';
import { startFromLine, workflowNames } from './run-command';
import { list as listWorkflows } from '../workflows/io';
import { cardOnTop, cardsIn, runTabs } from './tabs/store';
import { newTerminal } from './tabs/terminal';
import { PausedBanner } from './limits/paused-banner';
import type { AgentFacts } from './rail/roster';
import { roster } from './rail/roster';
import { Rail, RAIL_WIDTH } from './rail/rail';
/* NAPIS ZAPROSZENIA JEDZIE ZE STAŁEJ PRZEŁĄCZNIKA, nie z literału tutaj: „dodaj zakres" ma
 * w całej aplikacji jedno brzmienie, a dwie kopie tego samego zdania rozjeżdżają się przy
 * pierwszej zmianie i wtedy odmowa odsyła do przycisku o innej nazwie (niezmiennik 13).
 * Import idzie w tę stronę bez cyklu: przełącznik importuje `./folders`, a nie ten plik. */
import { FIRST_INVITE } from '../../ui/shell/workspace-switcher';
import { Start } from './start';
import { stripFor } from './strip/model';
import { Strip } from './strip/strip';
import { CloseConfirm } from './tabs/picker';
import { TabBar } from './tabs/tab-bar';

/** Trzy rzędy ekranu pracy: karty, pasek loadoutu, praca (makieta, `data-screen="work"`). */
const SCREEN_ROWS = 'auto auto minmax(0,1fr)';

/** Reguła `.work` z makiety: strumień bierze resztę, lista agentów swoje 268 px. */
const WORK_COLUMNS = `minmax(0,1fr) ${String(RAIL_WIDTH)}px`;

/** Reguła `.feedcol` z makiety: historia przewija się, TERAZ i wiersz wejścia stoją na dole. */
const FEED_ROWS = 'minmax(0,1fr) auto auto';

/**
 * Co w kolumnie strumienia zatrzymuje kursor u siebie.
 *
 * Kontrolka, w którą człowiek CELOWAŁ, ma dostać klawiaturę — wiersz wejścia, który zabiera
 * kursor po KAŻDYM kliknięciu w tej kolumnie, psuje każdy przycisk, jaki strumień rysuje
 * (dziś `Jump to newest`, `+` przy wyjściu, które padło, i przyciski odpowiedzi na pytanie).
 * Kryterium `e2e/tests/terminal-behaves.spec.ts` stoi po obu stronach tej reguły: raz pyta, czy
 * kursor WRACA, i raz, czy ZOSTAJE tam, gdzie kliknięto.
 *
 * Lista jest zbiorem rzeczy skupialnych, nie listą naszych komponentów: `[tabindex]` łapie
 * wszystko, co ktoś kiedyś uczyni skupialnym bez pytania tego pliku o zgodę.
 */
const KEEPS_THE_CARET = 'a, button, input, select, textarea, [contenteditable], [tabindex]';

/* Przycisk podstawowy z DESIGN §6 — te same cztery tokeny, co w `src/ui/primitives/empty-state.tsx`:
 * `--accent` na tle, `--bg` na tekście, wysokość 36 px. Akcent jest jedynym kolorem interaktywnym
 * (DESIGN §3), a na ekranie bez zakresu to jest jedyna kontrolka, której człowiek może użyć —
 * Start stoi wtedy wygaszony, bo bieg bez folderu odmawia. */
const INVITE = 'h-primary rounded-sm bg-accent px-4 text-ui text-bg';

/* Ta sama migawka dla okna i dla renderu serwerowego. Model nie ma stanu „po stronie serwera":
 * `renderToStaticMarkup` widzi po prostu bieg, którego jeszcze nie ma. */
function currentView(): FeedView {
  return runFeed.view;
}

/* Magazyn kart mieszka w `./tabs/store` (dawne `./workspaces-store`, przepisane 2026-08-18 na
 * karty BIEGÓW). Re-eksport pod starą nazwą zostaje, bo dwa testy kryteriów importują
 * `workspaces` WŁAŚNIE stąd (`tabs-switch-workspaces.test.tsx`, `entry-row.test.tsx`),
 * a przepisanie ich importu przy okazji przeprowadzki byłoby zmianą cudzego kryterium. */
export { runTabs as workspaces } from './tabs/store';

/**
 * Kroki biegu jako fakty o agentach, których szuka lista agentów.
 *
 * PODPIS AGENTA W STRUMIENIU TO NAZWA KROKU, i to nie jest domysł: `src-tauri/src/commands/run.rs`
 * uruchamia pompę zdarzeń jako `forward(…, self.plan.steps[id].name.clone())`, więc pole `agent`
 * każdej linii niesie nazwę kroku. Dopasowanie po niej jest JEDYNYM prawdziwym połączeniem
 * między planem a strumieniem, jakie w tych danych istnieje.
 *
 * ROLA ZOSTAJE PUSTA — zgłoszenie, nie przeoczenie. `Step` w `src/state/run.ts` niesie `id`,
 * `name` i `state`, i ani jedno z tych pól nie mówi, po co ten agent jest. Wpisanie tam nazwy
 * kroku drugi raz albo zmyślonego zdania byłoby relacją, której w danych nie ma (niezmiennik 17),
 * a kafelek pustego slotu po prostu nie rysuje.
 */
function factsOf(steps: readonly Step[]): readonly AgentFacts[] {
  return steps.map((step) => ({ id: step.name, name: step.name, role: '', step: step.state }));
}

/**
 * Kiedy wraca limit dostawcy — albo `null`, kiedy bieg wysyła.
 *
 * Pauza jest stanem, którego nie ma w żadnym polu magazynu: niesie ją linia `problem` z pola
 * `resetsAt` (`src/ipc/types.ts`, lustro `engine/line.rs`). Bieg CZEKA dokładnie wtedy, gdy ta
 * linia jest ostatnią rzeczą, jaka się wydarzyła — pierwsza linia po niej znaczy, że dostawca
 * znowu odpowiada. Wersja, która pamięta pauzę do końca biegu, zostawia na ekranie zdanie
 * o czekaniu przy agencie, który od dziesięciu minut pisze kod.
 */
function pausedUntil(lines: readonly FeedLine[]): number | null {
  const last = lines.at(-1);
  if (last === undefined || last.kind !== 'problem') return null;
  return last.resetsAt;
}

export default function Run(): ReactElement {
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);
  const run = useSyncExternalStore(useRun.subscribe, useRun.getState, useRun.getState);
  const tabs = useSyncExternalStore(runTabs.subscribe, runTabs.getState, runTabs.getState);
  /* ZAKRES CZYTAMY TĄ SAMĄ DROGĄ, CO POZOSTAŁE MAGAZYNY — i to jest jedyne miejsce, w którym
   * ten ekran pyta „gdzie pracujemy". Odpowiedź mieszka w `src/state/workspaces.ts`
   * (`activeWorkspace()`), a ekran jej nie kopiuje: bierze pole `activeId` z tej samej migawki,
   * z której rysuje resztę, żeby przerysowanie po przełączeniu było jednym renderem, a nie dwoma
   * z niespójnym stanem pośrednim. */
  const scopes = useSyncExternalStore(
    useWorkspaces.subscribe,
    useWorkspaces.getState,
    useWorkspaces.getState,
  );
  /* Zakres, w którym człowiek pracuje — CAŁY, nie sam folder, bo `＋` potrzebuje też jego nazwy:
   * świeży terminal nie ma jeszcze workflow, więc mówi o sobie tym słowem, które człowiek sam
   * wpisał w bocznym menu. Nazwa folderu wyliczona z jego ścieżki byłaby drugą odpowiedzią na
   * pytanie „jak nazywa się ten projekt" (niezmiennik 13). */
  const scope = scopes.all.find((one) => one.id === scopes.activeId) ?? null;
  const folder = scope?.folder ?? null;

  /* Jedno miejsce na to, co Loadout odpowiedział o folderze albo o zatrzymaniu wywołanym
   * z wiersza wejścia. Cicha porażka wygląda dokładnie jak martwa kontrolka. */
  const [said, setSaid] = useState<string | null>(null);

  /* Uchwyt do pola wiersza wejścia — po to, żeby kliknięcie w strumień mogło mu ODDAĆ kursor.
   * Powód w całości przy `caretBackToTheField`. */
  const field = useRef<HTMLInputElement>(null);

  /* Ta sama liczba, którą pokazuje kontrolka startu — jeden fakt, jedno miejsce (niezmiennik 13).
   * Gdyby ekran trzymał własną kopię, pasek kart mówiłby „of 3", kiedy suwak stoi na 8. */
  const atOnce = useSyncExternalStore(subscribeToAtOnce, atOnceNow, atOnceNow);

  const strip = useMemo(() => stripFor(run.workflow, run.steps), [run.workflow, run.steps]);
  const cards = useMemo(() => roster({ view, agents: factsOf(run.steps) }), [view, run.steps]);

  /* KTO SŁUCHA, czyli czyją nazwą można zaadresować zdanie z wiersza wejścia. Rozstrzygnięcie
   * właściciela 2026-08-19: „powinienem wiedzieć co piszę".
   *
   * 2026-08-20 — TA LISTA PRZESTAŁA BYĆ LISTĄ ODBIORCÓW I STAŁA SIĘ LISTĄ ADRESÓW. Do tego dnia
   * jej niepustość WYSTARCZAŁA, żeby zdanie poszło do agenta; teraz zdanie idzie do lidera,
   * dopóki nazwa z tej listy nie stanie na początku linii (`./addressee.ts`). Sam zbiór jest ten
   * sam i musi być ten sam — to z niego Rust buduje swoją odmowę.
   *
   * Kroki w stanie `running` i tylko one: to jest dokładnie ten sam zbiór, z którego bieg buduje
   * swoją odpowiedź po stronie Rusta (`RunControl::step_can_hear` rejestruje głos kroku, kiedy on
   * rusza, i zdejmuje go, kiedy schodzi). Osobne pole „czy można pisać" byłoby drugim opisem tego
   * samego faktu i pierwszą rzeczą, która rozjechałaby się z odmową (niezmiennik 13).
   *
   * Nazwa kroku, bo to nią człowiek adresuje zdanie i to ona stoi na kafelku szyny oraz w podpisie
   * każdej linii tego kroku. */
  const listening = useMemo(
    () => run.steps.filter((step) => step.state === 'running').map((step) => step.name),
    [run.steps],
  );

  /* KARTY TEGO ZAKRESU, i tylko tego. Bieg z innego zakresu nie znika i nie zwalnia — ma tylko
   * swoją kartę tam, gdzie pracuje (rozstrzygnięcie właściciela: przełącznik w bocznym menu
   * ORAZ karty w środku ekranu). Filtr jest funkcją czystą w `./tabs/store`, żeby dało się go
   * osądzić bez okna. */
  const shown = useMemo(() => cardsIn(tabs.tabs, folder), [tabs.tabs, folder]);
  /* KTÓRA KARTA JEST NA WIERZCHU — z tych, które WIDAĆ, i to jest to samo wyrażenie, którym
   * rozstrzyga to rejestr strumienia (`./feed/live`, `runFeed`). Jedna odpowiedź na jedno pytanie
   * (niezmiennik 13): dwie kopie dałyby pasek podświetlający jedną kartę nad historią należącą
   * do drugiej — i wyglądałoby to jak lider, który odpowiada nie na to, o co pytano. */
  const onTop = useMemo(
    () => cardOnTop(tabs.tabs, tabs.activeId, folder),
    [tabs.tabs, tabs.activeId, folder],
  );

  /* INSTANCJA COMPOSERA, nie tylko kanoniczny klucz feedu. Zwykłe `null → folder` zachowuje
   * rozmowę, bo oba zapisy znaczą domyślny terminal folderu. Ruch odwrotny jest inny: dzieje się
   * po `Close`, które woła `close_terminal`, więc ten sam napis klucza nazywa już NOWĄ rozmowę.
   * Generacja niesie tę różnicę bez zmiany kontraktu kart (poza OWNS T-34). */
  const canonicalEntryTerminal = onTop ?? folder ?? '';
  const entryInstance = useRef({
    folder,
    onTop,
    canonicalTerminal: canonicalEntryTerminal,
    generation: 0,
  });
  const previousEntry = entryInstance.current;
  const conversationChanged =
    previousEntry.folder !== folder ||
    previousEntry.canonicalTerminal !== canonicalEntryTerminal ||
    (previousEntry.onTop !== null && onTop === null);
  entryInstance.current = {
    folder,
    onTop,
    canonicalTerminal: canonicalEntryTerminal,
    generation: previousEntry.generation + (conversationChanged ? 1 : 0),
  };
  const entryKey =
    (folder ?? '') +
    '\u0000' +
    canonicalEntryTerminal +
    '\u0000' +
    String(entryInstance.current.generation);

  /* CZEGO TU NIE MA: przestawiania sesji przy przełączeniu zakresu. Oba magazyny robią to same
   * i każdy z nich słucha magazynu zakresów u siebie — `runFeed` w `./feed/live`, `useRun`
   * w `src/state/run.ts`. Trzecia droga do tej samej zmiany, dopisana tutaj z efektu, byłaby
   * drugim miejscem, w którym mieszka odpowiedź „którą sesję widać" (niezmiennik 13), i to
   * gorszym: efekt nie odpala się w renderze statycznym, więc test widziałby inną sesję niż
   * okno. */

  /* LICZBA ZYWYCH AGENTOW WRACA DO KARTY, i to nie jest ozdoba.
   *
   * `WorkspaceTab.agents` bylo pisane tylko przy zakladaniu karty i zawsze zerem, wiec
   * `requestClose` zawsze wchodzil w galaz „nic tu nie chodzi": karta z ZYWYM biegiem znikala
   * bez pytania i bez `cancel(id)`, czyli bieg zostawal osierocony i dalej pali limit
   * (niezmiennik 6 nazywa to bledem finansowym, nie higienicznym). Potwierdzenie zamkniecia
   * — zamontowane i przetestowane — bylo przez to kodem NIEOSIAGALNYM.
   *
   * Zrodlem jest ta sama lista, ktora rysuje szyne, wiec liczba na karcie i kafelki obok siebie
   * nie moga sie rozjechac (niezmiennik 13).
   *
   * 2026-08-20 (T-71) — PISZEMY DO KARTY NA WIERZCHU, i to jest poprawka do zdania, ktore stalo
   * tu wczesniej („do karty tego ZAKRESU, nie do karty na wierzchu"). Tamto bylo prawdziwe,
   * dopoki w zakresie mogla stac najwyzej jedna karta: „karta zakresu" i „karta na wierzchu"
   * byly wtedy tym samym. Od dziś nie sa. Szyna rysuje sie z `runFeed.view`, czyli z sesji
   * TERMINALU NA WIERZCHU, wiec `cards.length` jest liczba o nim — a wpisana na karte folderu
   * bylaby zdaniem o jednej karcie policzonym z danych drugiej (niezmiennik 17).
   *
   * W trakcie biegu te dwie odpowiedzi i tak sie zgadzaja: karte biegu zaklada `cardForRun`
   * i STAWIA JA NA WIERZCHU, wiec `onTop` jest wtedy karta biegu. Roznica widac tylko wtedy,
   * kiedy nic nie biegnie, a czlowiek rozmawia w swiezym terminalu — i wtedy liczba nalezy do
   * tego terminalu. */
  useEffect(() => {
    if (onTop === null) return;
    runTabs.getState().setAgents(onTop, cards.length);
  }, [onTop, cards.length]);
  const running = run.workflow !== '';

  /* NAZWY WORKFLOW DO PODPOWIEDZI POD `/run` — zgłoszenie właściciela 2026-08-19: „powinno
   * podpowiadać jakie workflow, tam podpowiadajka powinna być". Makieta obiecuje to samo w drugiej
   * linii wiersza wejścia („Tab completes a workflow").
   *
   * Czytane przy wejściu na sekcję, tym samym adapterem, którego używa lista wyboru obok Startu
   * i sekcja Workflow — pliki są prawdą (niezmiennik 4), a lista trzymana między wejściami
   * podpowiadałaby nazwę workflow skasowanego obok. Cisza przy odmowie jest tu POPRAWNA: brak
   * podpowiedzi jest niedogodnością, a `/run` i tak odmówi zdaniem, które wymienia nazwy. */
  /* ROZMOWA DOSTAJE SWÓJ STRUMIEŃ przy wejściu na sekcję — lider jeszcze nie wstaje.
   *
   * Otwarcie zakłada wyłącznie kanał do okna; rozmowa u dostawcy powstaje przy PIERWSZYM zdaniu
   * (`commands::chat::Threads::say_in`), bo tura wystartowana przy montażu ekranu jest turą, za
   * którą ktoś płaci, choć nikt o nic nie zapytał.
   *
   * ZALEŻNE OD KARTY, NIE TYLKO OD ZAKRESU, i to jest zmiana z 2026-08-20. Rozmowa należy do
   * TERMINALU: rejestr po tamtej stronie potrzebuje wpisu na tę kartę, zanim padnie w niej
   * pierwsze zdanie — bez niego odmawia, bo wątek bez kanału jest wątkiem, którego wierszy nikt
   * nie odbiera. Wołanie zależne tylko od folderu zostawiałoby drugą kartę bez strumienia,
   * a jej pierwsze zdanie odbijałoby się zdaniem o niegotowym liderze.
   *
   * WOŁANE PONOWNIE NIC NIE KOŃCZY — przekierowuje wiersze na nowy kanał i zostawia rozmowę
   * (`ipc::open_chat`, akapit o dzienniku z 2026-08-19). Dlatego powrót na kartę, na której już
   * się rozmawiało, jest kolejną turą tej samej rozmowy, a nie nową.
   *
   * Cisza przy odmowie jest poprawna — pierwsze zdanie i tak wróci z odmową, która nazywa
   * następny ruch. */
  useEffect(() => {
    openChat(folder, onTop).catch(() => {
      /* Świadomie bez zdania na ekranie: zdanie o niedostępnej rozmowie ma sens dopiero wtedy,
       * gdy człowiek do niej napisze — a wtedy przyjdzie z `say_to_orchestrator`. */
    });
  }, [folder, onTop]);

  const [namesToRun, setNamesToRun] = useState<readonly Named[]>([]);
  useEffect(() => {
    let alive = true;
    listWorkflows()
      .then((entries) => {
        if (alive) setNamesToRun(workflowNames(toChoices(entries)));
      })
      .catch(() => {
        /* Świadomie bez zdania na ekranie: o nieczytelnym katalogu mówi już kontrolka startu
         * (`start.tsx`), a dwa zdania o jednym fakcie to dwa miejsca prawdy. */
      });
    return () => {
      alive = false;
    };
  }, []);

  /**
   * `/open` z wiersza wejścia: pyta o folder i dokłada go jako ZAKRES.
   *
   * 2026-08-18 — CO TA FUNKCJA ROBIŁA WCZEŚNIEJ. Otwierała kartę na wybranym folderze, bo karta
   * znaczyła folder. Karty znaczą teraz biegi, a folder pracy jest zakresem — więc ta czynność
   * kończy się tam, gdzie mieszka ta decyzja: w magazynie zakresów, tym samym, którego używa
   * przełącznik w bocznym menu (niezmiennik 13). Nazwa nadana automatycznie jest nazwą folderu;
   * człowiek zmienia ją w bocznym menu, gdzie jest na to pole.
   *
   * ODMOWA MA GŁOS. `add` oddaje `false` i zostawia zdanie w `said` magazynu — słowo w słowo od
   * Rusta, bo to on wie, czego nie ma na dysku. Cisza po kontrolce wygląda dokładnie jak
   * kontrolka martwa, a to jest defekt, z którego wzięło się całe to zadanie (niezmiennik 16).
   */
  function openFolder(): void {
    setSaid(null);
    chooseWorkingFolder()
      .then(async (picked) => {
        /* Anulowanie okna wyboru jest wartością, nie błędem (niezmiennik 7): człowiek się
         * rozmyślił i nie ma o czym mówić. */
        if (picked === null) return;
        const done = await useWorkspaces.getState().add(folderName(picked), picked);
        if (!done) setSaid(useWorkspaces.getState().said);
      })
      .catch((error: unknown) => {
        setSaid(why(error, 'Loadout could not open the folder chooser.'));
      });
  }

  /**
   * `＋` na pasku kart: NOWY TERMINAL w projekcie, który już wybrano.
   *
   * ZGŁOSZENIE, Z KTÓREGO TO WZIĘŁO SIĘ W CAŁOŚCI (właściciel, 2026-08-20): „jak klikam plusik to
   * powinno po prostu odpalać nowy nasz terminal i sobie tam możemy kolejne workflow w naszym
   * scope co mamy zaznaczone, a nie tak jak teraz że scope wybieramy znowu".
   *
   * STAN BYŁ GORSZY, NIŻ BRZMI ZGŁOSZENIE. `＋` wołał [`openFolder`], czyli systemowe okno wyboru
   * katalogu, a wybór kończył się dołożeniem nowego ZAKRESU — który od razu stawał się aktywny.
   * Pasek pokazuje karty aktywnego zakresu, a w świeżym nie ma żadnej: kliknięcie w `＋` nie
   * dokładało więc karty **nigdy**, tylko wymieniało projekt i opróżniało pasek.
   *
   * DECYZJA MIESZKA TUTAJ, A NIE W PASKU, bo to ten ekran wie, czy jest gdzie postawić terminal.
   * Pasek dostaje jeden handler i nie zna pojęcia zakresu (`./tabs/tab-bar.tsx`).
   *
   * BEZ ZAKRESU DALEJ PYTAMY O FOLDER, i to nie jest wyjątek dla wygody: terminal bez miejsca
   * pracy nie ma gdzie stanąć, a karta, której praca nie ma domu, jest kropką nad folderem,
   * którego nie ma (niezmiennik 17). Pytanie o folder jest wtedy jedyną uczciwą odpowiedzią —
   * i jest tą samą czynnością, co zaproszenie na pustym ekranie i `/open` w wierszu wejścia.
   *
   * KURSOR WRACA DO POLA, bo otwarcie terminalu JEST prośbą o to, żeby w nim pisać. Przeglądarka
   * zostawia ognisko na przycisku, który nacisnięto, więc bez tej linii każdy nowy terminal
   * kosztowałby jedno kliknięcie więcej przed pierwszym zdaniem — czyli dokładnie tę wadę, którą
   * właściciel zgłosił tego samego dnia o wierszu wejścia (T-58 AC-3).
   */
  function newTerminalHere(): void {
    setSaid(null);
    if (scope === null || folder === null) {
      openFolder();
      return;
    }
    /* `open` z fabryki dokłada kartę I STAWIA JĄ NA WIERZCHU — jednym `set`, więc nie ma chwili,
     * w której karta już stoi, a wierzch należy jeszcze do poprzedniej. Człowiek, który poprosił
     * o nowe miejsce do pracy, patrzy na nie, a nie na to, co było przedtem. */
    runTabs.getState().open(newTerminal(folder, scope.name));
    field.current?.focus();
  }

  /**
   * Proza z wiersza wejścia → lider, albo krok, którego człowiek nazwał na początku linii.
   *
   * Zdanie odmowy WRACA do wiersza, a nie ląduje w `said` tego ekranu, i to jest jedyne miejsce,
   * gdzie te dwa kanały świadomie się rozchodzą: odpowiedź na to, co człowiek właśnie napisał,
   * ma stanąć pod polem, w które pisał. Zdanie o folderze albo o Starcie mówi o ekranie i stoi
   * pod paskiem.
   */
  async function sayIt(
    text: string,
    images: readonly ConversationImage[] = [],
  ): Promise<string | null> {
    /* KOMU DORĘCZYĆ, ROZSTRZYGA `addressee.ts` — i to jest ZMIANA POLITYKI z 2026-08-20, nie
     * przeprowadzka warunku.
     *
     * CO BYŁO. Stał tu jeden warunek: `listening.length > 0` → zdanie idzie do pracującego
     * agenta. Skutek zgłosił właściciel: „proza w trakcie biegu znika z rozmowy z liderem, bo
     * leci do pracującego agenta". Lider milczał przez cały bieg, czyli dokładnie wtedy, kiedy
     * człowiek chce zapytać, co się właściwie dzieje — i wysłanie tego pytania komuś, kto pisze
     * kod, jest zarazem bezużyteczne i płatne.
     *
     * CO JEST. Zdanie bez ukośnika idzie do lidera ZAWSZE, a do kroku wyłącznie wtedy, gdy jego
     * nazwa stoi na początku linii. Konwencja nie jest wymyślona tutaj: tak każe adresować Rust,
     * kiedy pracuje kilku (`RunError::SeveralAreWorking`), więc to samo słowo znaczy to samo po
     * obu stronach granicy.
     *
     * ŻADEN Z TYCH DWÓCH NIE URUCHAMIA BIEGU — rozstrzygnięcie właściciela 2026-08-19: „tylko
     * komendy determinują akcje workflow".
     *
     * Rozbiór mieszka w czystym module, a nie w tym ciele, bo to repo nie ma jsdom: polityka
     * zamknięta w `sayIt` byłaby kodem, którego nie umie dotknąć żadne kryterium. Wiersz mówi
     * POD POLEM, do kogo trafi zdanie, zanim ktokolwiek naciśnie Enter (`entry/entry.tsx`,
     * `whereItGoes`), i czyta to z tej samej listy pracujących kroków. */
    /* KTÓRY TERMINAL TO MÓWI I KOGO CZŁOWIEK WSKAZAŁ NA LIDERA — dwie wartości, które od
     * 2026-08-20 dojeżdżają do Rusta, i bez których żadna z nich nie miała nośnika.
     *
     * `onTop` odróżnia dwie karty jednego projektu. Bez niego rejestr wątków oddaje im JEDNĄ
     * rozmowę: człowiek pisze w lewej karcie, a odpowiedź pojawia mu się w prawej.
     *
     * `lead()` jest wskazaniem z paska (`./lead.ts`, kontrolka w `./start.tsx`) i do tego dnia
     * NIE MIAŁO DRUTU: wybór żył w oknie, a Rust rozmawiał zaszytym Claude'em, kimkolwiek by ten
     * wybór nie był. Czytamy je w chwili wysyłki, nie z migawki renderu — zdanie ma pójść do tego
     * lidera, którego widać na pasku teraz. */
    const going = addresseeOf(text, listening);
    /* `say_to_agent` nie ma nośnika obrazów. Jawna odmowa przed IPC jest węższa i uczciwsza
     * niż ciche zdjęcie załączników ze szkicu adresowanego nazwą żywego kroku. */
    if (images.length > 0 && going.to === 'agent') return IMAGES_TO_LEAD_ONLY;
    try {
      await (going.to === 'agent'
        ? sayToAgent(going.text, going.agent)
        : sayToOrchestrator(going.text, folder, onTop, lead(), images));
      return null;
    } catch (error: unknown) {
      /* Odrzucenie obrazu może nieść stderr vendora albo fragment prywatnego payloadu. Dla tej
       * granicy odpowiedź jest stała; ścieżka tekstowa zachowuje dotychczasowe `why`. */
      if (images.length > 0) return IMAGE_SEND_FAILED;
      return why(
        error,
        going.to === 'agent'
          ? 'Loadout could not pass that on to the agent.'
          : 'Loadout could not reach the lead agent.',
      );
    }
  }

  /**
   * Wiersz złożony przez OKNO → strumień tego zakresu.
   *
   * `feedFor(folder ?? '')`, nie `runFeed`: to jest ta sama sesja i ten sam sentinel pustego
   * napisu, którymi piszą obie pompy na granicy (`./io.ts`, `start` i `openChat`), więc wiersz
   * wpisany tutaj stoi w historii w kolejności, w której się wydarzył. `runFeed` rozstrzyga sesję
   * W CHWILI WYWOŁANIA, czyli po zakresie AKTUALNIE widocznym — a linia należy do zakresu, w
   * którym ją wpisano, nawet jeśli człowiek przełączy się, zanim wróci odmowa.
   *
   * DO WIDOKU, NIGDY DO MAGAZYNU LINII (`runFor`). Ten wiersz nie jest zdarzeniem biegu: nie ma
   * go w `run.json`, nie przeżyje przeładowania okna i niesie to w swoim ujemnym identyfikatorze
   * (niezmiennik 4). Dopisany do okna linii udawałby zdarzenie, którego nie da się odtworzyć
   * z plików — a `pausedUntil` w tym pliku czyta z tego okna OSTATNI wiersz i zgadywałby po nim,
   * czy bieg czeka na limit dostawcy.
   */
  function showInStream(row: WindowLine): void {
    feedFor(folder ?? '').appendLines([row]);
  }

  /**
   * Kliknięcie w kolumnę strumienia oddaje kursor polu — chyba że celowało w kontrolkę.
   *
   * DRUGA POŁOWA WADY „kursor nie stoi w polu" (zgłoszenie właściciela 2026-08-20). Pole, które
   * startuje z ogniskiem i nigdy go nie odzyskuje, działa dokładnie raz: pierwsze kliknięcie
   * gdziekolwiek w strumień odbiera klawiaturę i człowiek wraca do klikania w pole przed każdą
   * linią. Terminal tak się nie zachowuje.
   *
   * DWA WYJĄTKI, i oba są warunkiem, żeby ta wygoda nie zabrała czegoś cenniejszego:
   *   kontrolka   człowiek celował w przycisk i klawiatura ma zostać na nim ([`KEEPS_THE_CARET`]),
   *   zaznaczenie skupienie pola KASUJE zaznaczenie w dokumencie, a wyjście polecenia, które
   *               padło, jest w tym widoku wartością do skopiowania (`feed/line.tsx`,
   *               `data-copyable`) — kursor wracający po zaznaczeniu logu zabierałby ten log.
   *
   * `onClick`, nie `onMouseDown`: ognisko zabrane przy WCIŚNIĘCIU przerywa zaznaczanie w połowie
   * ruchu myszy, czyli psuje tę samą rzecz, której pilnuje warunek wyżej.
   */
  function caretBackToTheField(event: MouseEvent<HTMLDivElement>): void {
    if (event.target instanceof Element && event.target.closest(KEEPS_THE_CARET) !== null) return;
    if (window.getSelection()?.isCollapsed === false) return;
    field.current?.focus();
  }

  /** Zatrzymanie z wiersza wejścia. `null`, kiedy nic nie biegnie — wtedy nie ma czego zatrzymać. */
  function stopRun(): void {
    setSaid(null);
    stop().catch((error: unknown) => {
      setSaid(why(error, 'Loadout could not stop the run.'));
    });
  }

  return (
    <section className="flex h-full min-h-0 flex-col">
      {/* OSOBNEGO PASKA NAGLOWKA TU NIE MA, i to jest naprawa, nie przeoczenie.
       *
       * Makieta na ekranie pracy (`data-screen="work"`) nie ma zadnego naglowka sekcji —
       * ekran przechodzi wprost w pasek kart. Wlasny rzad 52 px stal tu do 2026-08-18 i byl
       * zapisanym dlugiem: `docs/ARCHITECTURE.md` §7 daje 96 px nad pierwsza trescia, karty
       * biora 34, pasek loadoutu 56, a ten pasek 52 — czyli sam sufit byl przekroczony,
       * zanim doliczylo sie cokolwiek innego. Zmierzone na zywym oknie: 284 px.
       *
       * Rozstrzygniecie jest tym, ktore proponuje tamten paragraf: nazwa sekcji wchodzi
       * W pasek loadoutu (`strip/strip.tsx`, stopien `.strip .title` z makiety). `<h1>` dalej
       * istnieje w drzewie — wymaga go `e2e/tests/sections-mount.spec.ts`, bo bez naglowka
       * „cos sie zamontowalo" nie odpowiada na pytanie, na ktorej sekcji stoisz — tylko nie
       * kosztuje juz osobnego rzedu. */}
      <div className="grid min-h-0 flex-1" style={{ gridTemplateRows: SCREEN_ROWS }}>
        <TabBar
          tabs={shown}
          activeId={onTop}
          /* TRZY LICZBY, TRZY PRAWDZIWE ŹRÓDŁA — i ani jednej zgadniętej.
           *
           * Do 2026-08-18 stały tu trzy zaszyte zera i zdanie „N of M slots in use" nie mogło się
           * pokazać nigdy. Dwie z nich okno znało od rana: ilu agentów pracuje (ta sama lista,
           * która rysuje szynę — więc liczba na pasku i kafelki obok siebie nie mogą się
           * rozjechać, niezmiennik 13) i ile naraz wybrał człowiek (`limits/chosen.ts`).
           *
           * TRZECIA JEST TERAZ POLICZONA, nie zgadnięta. Nośnikiem jest stan kroku `ready`
           * („gotowy, jeszcze bez permitu" [ARCHITECTURE §5]), który dowozi wiersz `stepState`
           * i przepisuje `src/state/run.ts`. `waitingWhere` oddaje `null`, kiedy nikt nie czeka
           * albo kiedy nie wiadomo, jak nazwać miejsce — a zdanie o kolejce, której nie ma,
           * jest gorsze niż brak zdania (niezmiennik 17). */
          busy={cards.length}
          atOnce={atOnce}
          waitingIn={waitingWhere(run.steps, run.folder ?? folder)}
          onSelect={tabs.activate}
          onClose={tabs.requestClose}
          /* `＋` OTWIERA TERMINAL, nie okno wyboru katalogu — powód w całości stoi przy
             `newTerminalHere`. Nazwa propsa jest zapisanym długiem i jest zgłoszona przy
             `TabBarProps.onOpenFolder`; zaproszenie na pustym ekranie niżej i `/open` w wierszu
             wejścia wołają dalej `openFolder`, bo one naprawdę pytają o folder. */
          onOpenFolder={newTerminalHere}
        />

        <div className="flex shrink-0 flex-col gap-2">
          {/* Nazwa sekcji z REJESTRU, nie literalem: `src/ui/sections.tsx` jest jedynym
                miejscem, w ktorym mieszka nazwa sekcji, i to samo zdanie czyta boczne menu.
                Napis „Run" wpisany tutaj rozjechalby sie z nawigacja przy pierwszej zmianie
                brzmienia (niezmiennik 13). */}
          <Strip
            strip={strip}
            heading={sectionEntry('run').label}
            /* KONTROLKI BIEGU STOJĄ W PASKU, w jego prawej grupie — powód (zmierzone 189 px
                 chrome przy sufcie 96) stoi przy `StripProps.controls`. Zdanie o tym, czego nie
                 udało się zacząć, wraca TUTAJ i ląduje w jedynym slocie tego ekranu: dwa
                 miejsca na „co powiedział Loadout" to dwa zdania sprzeczające się o to samo
                 (niezmiennik 13). */
            controls={
              <div className="flex min-w-0 items-center gap-2">
                {/* Diagnostyka należy do aktywnego workspace i stoi przy czynnościach tego
                    ekranu. W fazie `before` komponent jest pustym szkieletem: prawdziwy mount
                    istnieje, a kryterium pada na braku kontrolki, nie brakującym imporcie. */}
                <Diagnostics folder={folder} />
                <Start running={running} onSaid={setSaid} />
              </div>
            }
          />
          {/* ZAPROSZENIE, KIEDY NIE MA GDZIE PRACOWAĆ — i to jest jedyny przycisk, jaki ten
                ekran ma sam z siebie.

                2026-08-18: `＋` zniknął z paska kart (powód w `tabs/tab-bar.tsx`), a bez zakresu
                Start jest wygaszony i wygaszony musi zostać — bieg bez folderu odmawia. Świeży
                ekran Run zostawał wtedy BEZ ANI JEDNEJ czynnej kontrolki do klikania, czyli
                w dokładnie tym stanie, który T-39 AC-6 zmierzył jako defekt („one button,
                `Start`, and it refused") i który DESIGN §6 nazywa komunikatem o braku danych
                zamiast zaproszeniem.

                Ten przycisk NIE jest drugim przełącznikiem zakresów: nie wybiera zakresu i nie
                pokazuje listy — robi dokładnie to samo, co `/open` w wierszu wejścia, czyli
                pyta o folder i dokłada go do jedynego magazynu zakresów, jaki jest
                (niezmiennik 13). Napis jedzie ze stałej przełącznika, żeby zdanie z odmowy
                (`NO_FOLDER` w `./launch`), przycisk w bocznym menu i ten przycisk nazywały tę
                jedną czynność tym samym słowem.

                Znika, kiedy zakres już jest: wtedy pierwszą czynnością jest Start, a przycisk
                proszący o kolejny projekt na ekranie pracy przeszkadza w tym, co człowiek
                właśnie robi. */}
          {scopes.all.length === 0 ? (
            <button type="button" data-add-workspace className={INVITE} onClick={openFolder}>
              {FIRST_INVITE}
            </button>
          ) : null}

          {/* Jeden pasek na BIEG (niezmiennik 13). Komponent sam znika, kiedy nie ma pauzy. */}
          <PausedBanner
            run={{
              waitingUntil: pausedUntil(run.lines),
              steps: run.steps.map((step) => step.state),
            }}
          />

          {/* Odpowiedź TEGO ekranu — o folderze i o zatrzymaniu wywołanym z wiersza wejścia.
              Osobny znacznik od `data-said` w `start.tsx`: tamten mówi o kontrolce startu,
              ten o kartach, więc to są dwa różne fakty, a nie dwa miejsca na jeden
              (niezmiennik 13). Cicha porażka wygląda dokładnie jak martwa kontrolka. */}
          {said === null ? null : (
            <p data-screen-said className="text-body text-fail">
              {said}
            </p>
          )}
        </div>

        <div data-work className="grid min-h-0" style={{ gridTemplateColumns: WORK_COLUMNS }}>
          <div
            data-stream-column
            /* Kliknięcie w tę kolumnę oddaje kursor wierszowi wejścia — powód i dwa wyjątki
               stoją w całości przy `caretBackToTheField`. Handler wisi na kolumnie, a nie na
               całym ekranie: lista agentów obok ma własne kontrolki i nie ma prawa tracić
               kliknięcia na rzecz pola, w które nikt nie celował. */
            onClick={caretBackToTheField}
            className="grid min-h-0 min-w-0"
            style={{ gridTemplateRows: FEED_ROWS }}
          >
            <Feed
              view={view}
              portRef={attachPort}
              onToggle={runFeed.toggle}
              onAnswer={runFeed.answer}
              onJumpToNewest={runFeed.jumpToNewest}
            />
            <Now now={view.now} live={running} />
            <Entry
              /* SZKIC NALEŻY DO ROZMOWY, nie do całego ekranu. Zmiana folderu albo terminalu
                 odmontowuje jego właściciela: cleanup odcina blob URL, a nowe pole zaczyna
                 puste. To świadomy kontrakt „clear on switch" — mniejszy koszt niż screenshot
                 z projektu A wysłany przez Enter w projekcie B. Fallback terminalu i wyjątek
                 zamknięcia są składane raz, wyżej, w `entryKey`. */
              key={entryKey}
              onOpenFolder={openFolder}
              onStopRun={running ? stopRun : null}
              onSayToAgent={sayIt}
              /* `/run` idzie WPROST do polityki startu, bez przechodzenia przez ten komponent:
                 `startFromLine` czyta katalog workflow, rozbiera linię i woła `launchRun` z tym
                 samym limitem, który trzyma suwak obok Startu. Zdanie odmowy wraca do wiersza,
                 bo dotyczy tego, co człowiek właśnie napisał. */
              onRunWorkflow={startFromLine}
              /* Kto pracuje, żeby wiersz mógł powiedzieć POD polem, gdzie pójdzie zdanie —
                 zamiast pozwolić człowiekowi wysłać je w ciemno. */
              talkingTo={listening}
              workflows={namesToRun}
              /* ŚLAD PO KAŻDEJ WYSŁANEJ LINII. Wiersz składa `entry/echo.ts`, a ten ekran wie
                 tylko, DO KTÓREJ sesji strumienia on należy — powód przy `showInStream`. */
              onShowInStream={showInStream}
              /* Uchwyt do pola, żeby kliknięcie w tę kolumnę mogło oddać mu kursor. */
              fieldRef={field}
            />
          </div>

          <Rail cards={cards} />
        </div>
      </div>

      {/* Pytanie o zamknięcie karty, w której ktoś pracuje. Magazyn je stawia; bez tego jednego
          renderu byłoby to pytanie zadane w próżnię, a `×` gubiłby bieg po cichu. */}
      {tabs.pendingClose === null ? null : (
        <div className="fixed inset-0 grid place-items-center bg-bg/70">
          <CloseConfirm
            pending={tabs.pendingClose}
            onConfirm={() => {
              void tabs.confirmClose();
            }}
            onDismiss={tabs.dismissClose}
          />
        </div>
      )}
    </section>
  );
}
