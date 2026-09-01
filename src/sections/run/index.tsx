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
 * karty, pasek loadoutu, praca — a praca to dwie kolumny: strumień i druga kolumna o szerokości
 * `PLAN_WIDTH`. Deklaracje siatki stoją niżej jako stałe, żeby test mógł je porównać z regułami
 * `.work` i `.feedcol` przeczytanymi z makiety w tym samym biegu (T-37 AC-1 zrobił to samo
 * z regułą `.app`).
 *
 * 2026-08-31 — CO ZNIKŁO Z TEGO EKRANU I DLACZEGO. Ten sam fakt stał na nim TRZY RAZY naraz:
 * strumień mówił „Check — Ran the checks, they did not work", blok TERAZ pod strumieniem mówił
 * to samo, kafelek w prawej kolumnie mówił to po raz trzeci. Limit żywych regionów na fakt
 * wynosi 1 (niezmiennik 13). Zeszły więc trzy widoki i ani jeden moduł:
 *   `feed/now.tsx`      blok TERAZ. Poza duplikatem miał dwie własne wady: ROSNĄŁ z każdym
 *                       agentem, choć DESIGN §1 żąda stałej wysokości, a po końcu biegu
 *                       zdejmował nagłówek i zostawiał wiersze — więc stan sprzed zakończenia
 *                       wyglądał jak stan bieżący, tylko bez etykiety.
 *   kolumna agentów     `rail/rail.tsx` przestało być kolumną. Cała jej logika — `roster.ts`,
 *                       `card.ts`, `say.ts`, `colour.ts`, `again.ts`, `processes.ts` — została
 *                       co do linii i to ona karmi obraz planu.
 *   torek bloków        `strip/` przestało rysować drugi ciąg kroków; prawa grupa paska dostała
 *                       przez to całą wolną szerokość i Start przestał wyjeżdżać poza kadr.
 * Na ich miejsce wchodzi JEDEN obraz: kroki, strzałki i pozycje z pliku workflow (`graph/`).
 * Kiedy plik ich nie niesie — a plan jednego kroku, składany dla wpisanego pytania, nie niesie
 * — obraz MILCZY o kształcie i pokazuje listę kroków (reguła 17).
 *
 * NAGŁÓWEK SEKCJI zostaje mimo makiety, która na ekranie pracy nie ma żadnego. Wymaga go
 * `e2e/tests/sections-mount.spec.ts`: każdy ekran ma się nazwać własnym nagłówkiem, inaczej
 * „coś się zamontowało" nie odpowiada na pytanie, na której sekcji stoisz. Koszt jest zapisany
 * i nadal nie ma z czego zapłacony: ARCHITECTURE §7 daje 96 px nad pierwszą treścią, karty
 * biorą 34, pasek loadoutu 56, a ten pasek 52. Domknięcie należy do zadania, które posiada
 * pasek loadoutu — albo nazwa sekcji wchodzi W ten pasek, albo któryś z dwóch znika.
 *
 * SKĄD BIERZE SIĘ TREŚĆ. Z dwóch źródeł i każde odpowiada na inne pytanie. Model widoku
 * (`feed/live.ts`) trzyma wiersze historii, to, co każdy agent robi TERAZ, i przypięte pytanie;
 * magazyn (`state/run.ts`) trzyma okno linii oraz plan biegu — kroki, ich pozycje i strzałki
 * — z którego rysuje się obraz i liczy podpis paska. Oba są na
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
import type { MouseEvent, ReactElement, ReactNode } from 'react';

import { why } from '../../ipc/why';
import { sectionEntry } from '../../ui/sections';
import type { FeedLine, Step } from '../../state/run';
import type { Link } from '../../state/workflows';
import { runFor, useRun } from '../../state/run';
import { useSkills } from '../../state/skills';
import { useWorkspaces } from '../../state/workspaces';
import { addresseeOf } from './addressee';
import { saidOf } from './entry/echo';
import type { WindowLine } from './entry/echo';
import { IMAGE_SEND_FAILED, IMAGES_TO_LEAD_ONLY } from './entry/images';
import type { ConversationImage } from './entry/images';
import { Feed } from './feed/feed';
import { attachPort, feedFor, runFeed } from './feed/live';
import type { FeedView, NowZone, Question } from './feed/model';
import { Entry } from './entry/entry';
import { PastRuns } from './past/panel';
import { Diagnostics } from './diagnostics';
import { chooseWorkingFolder, folderName } from './folders';
import { openOneRun, theOneThatIsGoing } from './history-command';
import {
  answerTheLead,
  listRuns,
  openChat,
  readRun,
  sayToAgent,
  sayToOrchestrator,
  stop,
} from './io';
/* KIM JEST LIDER — jedno źródło, to samo, z którego czyta kontrolka w pasku (`./start.tsx`).
 * Ten ekran wskazania nie kopiuje i nie trzyma: pyta o nie w chwili wysyłki zdania. */
import { lead } from './lead';
import {
  atOnce as atOnceNow,
  budgetOfTheRun,
  subscribeToAtOnce,
  subscribeToBudget,
} from './limits/chosen';
import { waitingWhere } from './limits/waiting';
import type { Choice } from './choices';
import { WORKFLOW_LABEL, offerFor, toChoices, whoChoseIt, willRun } from './choices';
import type { Named } from './run-command';
import { startFromLine, workflowNames } from './run-command';
import { list as listWorkflows } from '../workflows/io';
/* ILU AGENTÓW LEŻY W BIBLIOTECE — jedno pytanie, przez tę samą krawędź, którą czyta sekcja
 * Agents (niezmiennik 23). Ekran Run nie trzyma agentów i nie ma po co: przewodnik pierwszego
 * uruchomienia potrzebuje LICZBY, a nie listy. `list` oddaje wyłącznie zdrowe definicje, więc
 * plik, którego nie da się wczytać, nie odhacza kroku „dodaj agenta" po cichu. */
import { list as listAgents } from '../agents/io';
import { cardOnTop, cardsIn, runTabs } from './tabs/store';
import { newTerminal } from './tabs/terminal';
import { PausedBanner } from './limits/paused-banner';
import type { AgentFacts } from './rail/roster';
import { agentStatusOf, roster } from './rail/roster';
import type { RailCard } from './rail/card';
import { sayAfterRunningAgain, StartedThings } from './rail/rail';
/* CO CZŁOWIEK URUCHOMIŁ KOMENDĄ — czytane TUTAJ, choć rysuje to komponent wyżej, bo o układ
 * obszaru pracy pyta ten ekran: pierwszy kafelek `/start` wraca do kolumny kroków i ta kolumna
 * musi wtedy istnieć. Jeden magazyn, dwóch czytelników, zero drugiej listy (niezmiennik 13). */
import { startedThings, subscribeToStarted } from './rail/processes';
import { AfterRun } from './graph/after-run';
import { RunGraph } from './graph/graph';
import { StepStream } from './graph/drawer';
import type { GraphStep, Plan as RunPlan } from './graph/model';
import {
  closeStepStream,
  openStepStream,
  openedStepStream,
  subscribeToOpenedStepStream,
} from './graph/opened';
import { AgentScreen } from './session/mount';
import { openAgent } from './session/open';
import { FirstRun, firstRunSteps, somethingIsLeft, welcomeIsTheWholeScreen } from './first-run';
import { ReadyToRun } from './ready';
import {
  lastRunIn,
  pickWorkflow,
  rememberAgents,
  rememberRuns,
  rememberWorkflows,
  subscribeToWhatIsReady,
  whatIsReady,
} from './whats-ready';
import { Start } from './start';
import { ReflectionToggle } from './reflection/toggle';
import { reflectionForRequestedRun, rememberReflectionChoice } from './requested';
import { headlineFor } from './strip/headline';
import { RunHead } from './strip/head';
import { Strip } from './strip/strip';
import { CloseConfirm } from './tabs/picker';
import { TabBar } from './tabs/tab-bar';

/** Trzy rzędy ekranu pracy: karty, pasek loadoutu, praca (makieta, `data-screen="work"`). */
const SCREEN_ROWS = 'auto auto minmax(0,1fr)';

/**
 * Szerokość kolumny planu w pikselach — 322 z reguły `.work` w makiecie.
 *
 * Liczba stoi TUTAJ od 2026-08-31, bo do tego dnia trzymał ją komponent, który tę kolumnę
 * wypełniał (`rail/rail.tsx`, `RAIL_WIDTH`), a tamta kolumna zniknęła. Drugiego literału tej
 * szerokości w repo nie ma i mieć nie może: makieta jest wyrocznią i czyta ją
 * `run-matches-mockup.test.tsx` w tym samym biegu (niezmiennik 13).
 *
 * 2026-08-31 — BYŁO 268 PO PRAWEJ, JEST 376 PO LEWEJ. Obie zmiany są jedną zmianą i obie
 * przyjechały z makiety: kolumna planu przestała być paskiem obok pracy i stała się ŚCIEŻKĄ,
 * po której czyta się bieg — a ścieżka z kartami kroków nie mieści się w 268 px (nazwa kroku
 * kończyła się wielokropkiem przy każdym kroku nazwanym pełnym zdaniem). Miejsce po lewej
 * bierze się z tego samego: ekran czyta się od lewej, a pierwsze pytanie brzmi „na czym stoi
 * ten bieg", nie „co przed chwilą powiedział agent".
 */
/* 2026-09-01 — 376 -> 322 px, na zgloszenie wlasciciela („a sama sekcje tez moz byc troche
 * wezsza"). Zmiescilo sie, bo tego samego dnia z tej kolumny zeszla RYNNA ZNACZNIKOW: 18 px
 * samej rynny plus 9 px przerwy do karty, a kafelek oddal jeszcze 6 px marginesu. Kolumna
 * wezsza o 54 px oddaje je strumieniowi, w ktorym czyta sie proze agentow. */
const PLAN_WIDTH = 322;

/** Reguła `.work` z makiety: ścieżka kroków bierze swoje 322 px, strumień resztę. */
const WORK_COLUMNS = `${String(PLAN_WIDTH)}px minmax(0,1fr)`;

/**
 * Dwa rzędy obszaru pracy: nagłówek biegu, a pod nim dwie kolumny.
 *
 * NAGŁÓWEK BIERZE OBIE KOLUMNY, dokładnie jak `.rhead` w makiecie — pas na całą szerokość nad
 * ścieżką kroków i strumieniem. Powód, dla którego stoi WEWNĄTRZ `[data-work]`, a nie nad nim,
 * stoi w całości w `./strip/head.tsx`: makieta wydaje w tym miejscu 222 px nad pierwszą treścią
 * przy suficie 96 px z `docs/ARCHITECTURE.md` §7, a tożsamość biegu jest treścią tego ekranu,
 * nie jego ramą. Rozbieżność zgłoszona właścicielowi z liczbami.
 *
 * PIERWSZY RZĄD ISTNIEJE ZAWSZE, także pusty, i to nie jest ozdoba: w siatce bez nazwanych
 * obszarów rząd bierze się z KOLEJNOŚCI dzieci, więc nagłówek znikający z drzewa wpycha ścieżkę
 * kroków do rzędu `auto`, a strumień do rzędu z resztą wysokości — czyli oddaje całą wysokość
 * kolumnie planu i zero strumieniowi. Pusty pojemnik mierzy zero pikseli i trzyma tę kolejność.
 */
const WORK_ROWS = 'auto minmax(0,1fr)';

/**
 * Jeden tor — cała tafla obszaru pracy dla jednego dziecka.
 *
 * STOI W OBU OSIACH PIERWSZEGO OTWARCIA, bo w obu mówi to samo i prawdę: ta siatka ma wtedy
 * jedno dziecko, jedną kolumnę i jeden rząd. Dlaczego pierwsze otwarcie w ogóle dostaje jeden
 * tor zamiast dwóch, stoi przy `welcomeIsTheWholeScreen` (`./first-run.tsx`).
 *
 * SZEROKOŚĆ ROBI RÓŻNICĘ, WYSOKOŚĆ DZIŚ NIE — i to jest zmierzone, nie założone. Podmiana toru
 * kolumn na `WORK_COLUMNS` wsadza powitanie w 376 px obok 800 px czerni i pada na kryterium
 * w `e2e/tests/the-first-open-fills-the-window.spec.ts`. Podmiana rzędów na `WORK_ROWS` nie
 * zmienia ani jednego piksela, bo kolumna strumienia niesie `min-h-0` i rząd `auto` rozciąga się
 * wtedy na całą wysokość tak samo jak `1fr`. Rzędy zostają zapisane wprost mimo to: układ ma
 * mówić, ile rzędów ma, a nie polegać na cudzej klasie w dziecku.
 */
const WHOLE_SURFACE = 'minmax(0,1fr)';

/** Reguła `.feedcol` z makiety: historia przewija się, odpowiedź Loadouta i wiersz wejścia
 * stoją na dole. */
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
  return steps.map((step) => ({
    id: step.name,
    name: step.name,
    role: '',
    step: step.state,
    /* Identyfikator jedzie obok nazwy: po nazwie rozpoznaje agenta strumień, a po kluczu
     * kafelka powtarza się krok (`io.rerunStep`). */
    stepId: step.id,
  }));
}

/**
 * Plan biegu tak, jak widzi go obraz w prawej kolumnie.
 *
 * TRZY ŹRÓDŁA, KAŻDE NA INNE PYTANIE, I ANI JEDNEGO ZGADNIĘTEGO POLA (niezmiennik 17):
 *   `steps`  co jest w planie, gdzie stoi każdy krok i jak się nazywa. Z pliku workflow.
 *   `cards`  kto ten krok robi i na czym stoi — z `roster()`, czyli ze STRUMIENIA. Kafelek
 *            agenta istnieje wtedy i tylko wtedy, gdy agent coś nadał, więc krok, o którym
 *            strumień milczy, bierze stan wprost z planu (`agentStatusOf`).
 *   `now`    co ten agent robi w tej chwili. To jest cała treść, którą do 2026-08-31 niósł
 *            osobny blok pod strumieniem — ta sama linia w drugim miejscu (niezmiennik 13).
 *
 * `at` JEDZIE NIETKNIĘTE, a `links` `null` zamienia się na pustą listę i to nie jest ta sama
 * odpowiedź co „ten plik nie ma strzałek": obraz milczy w obu wypadkach (pokazuje listę
 * kroków), więc różnica nie ma tu nośnika i nie ma prawa udawać, że ma. Rozróżnienie żyje
 * w magazynie (`state/run.ts`), gdzie jest prawdziwe.
 *
 * `who.name` ZOSTAJE PUSTE, KIEDY AGENT NAZYWA SIĘ TAK JAK KROK, i to jest naprawa, nie
 * oszczędność: podpis agenta w strumieniu JEST nazwą kroku (`commands/run.rs` woła
 * `forward(…, step.name)`), więc wpisany drugi raz stałby na kafelku dwa razy — raz jako
 * nagłówek, raz jako wykonawca, w dwóch krojach. Kwadrat tożsamości zostaje w obu wypadkach:
 * to on odróżnia agentów wzrokiem [DESIGN §3].
 *
 * CZWARTE ŹRÓDŁO, OD 2026-08-31: `asked`, czyli pytanie bez odpowiedzi. Trafia na TEN krok,
 * którego nazwą to pytanie jest podpisane — tym samym dopasowaniem, co reszta tego pliku, bo
 * podpis w strumieniu JEST nazwą kroku. Pytanie, którego nie da się przypisać do żadnego kroku
 * (lider, pod-agent rozpuszczony w biegu), nie ląduje nigdzie: karta zostaje wtedy na dole
 * strumienia, gdzie stała od zawsze. Brak kroku znaczy „nie wiemy, kto pyta", nigdy „nikt nie
 * pyta" (niezmiennik 17).
 */
function planFor(
  steps: readonly Step[],
  links: readonly Link[] | null,
  cards: readonly RailCard[],
  now: NowZone,
  pinned: Question | null,
): RunPlan {
  const spoke = new Map(cards.map((card) => [card.id, card]));
  const doing = new Map(now.rows.map((row) => [row.agent, row.text]));

  return {
    steps: steps.map((step): GraphStep => {
      const card = spoke.get(step.name);
      /* Żywe zdanie bije ostatnie: strefa TERAZ mówi, co się dzieje, a `say.ts` — co ten agent
       * powiedział z autorytetem, kiedy już nic się nie dzieje. Dwa różne pytania, jedna linia
       * na kafelku, więc pierwszeństwo musi być zapisane, a nie przypadkowe. */
      const says = doing.get(step.name) ?? card?.say.text ?? '';
      const asked = pinned !== null && pinned.agent === step.name ? pinned : undefined;
      return {
        id: step.id,
        name: step.name,
        status: card?.status ?? agentStatusOf(step.state),
        ...(card === undefined
          ? {}
          : { who: { name: card.name === step.name ? '' : card.name, square: card.square } }),
        ...(says === '' ? {} : { doing: says }),
        ...(step.at === undefined ? {} : { at: step.at }),
        ...(asked === undefined ? {} : { asked }),
      };
    }),
    links: links ?? [],
  };
}

/**
 * Strefa TERAZ dla obrazu, w którym nikt jeszcze nie pracuje.
 *
 * STAŁA, nie `{ rows: [], thinking: null }` wpisane w wyrażenie: `planFor` bierze tę strefę
 * przez `useMemo`, a świeży obiekt przy każdym renderze przeliczałby cały plan bez powodu —
 * czyli przerysowywał płótno React Flow za każdym drgnięciem jakiegokolwiek innego pola.
 */
const NOBODY_IS_WORKING: NowZone = { rows: [], thinking: null };

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

/**
 * HANDLER WYBORU WORKFLOW RAZEM ZE ŚLADEM PO NIM — jeden obiekt, więc jednego bez drugiego nie
 * da się na kontrolce zostawić.
 *
 * DLACZEGO ŚLAD W OGÓLE ISTNIEJE. To repo nie ma jsdom, a `renderToStaticMarkup` nie zapisuje
 * handlerów w markupie: kontrolka z handlerem i kontrolka bez niego dają CO DO BAJTA ten sam
 * napis. Sam atrybut byłby więc obietnicą, którą kryterium czyta ZAMIAST mechanizmu — a kontrolka
 * przyjmująca decyzję i wyrzucająca ją to dokładnie ta wada, którą to zadanie zamyka
 * (niezmiennik 16). Skoro oba pola jadą jednym obiektem, atrybut nie ma jak przeżyć skasowania
 * handlera.
 *
 * EKSPORTOWANY, żeby kryterium mogło ten handler ZAWOŁAĆ i zobaczyć, że nośnik naprawdę się po
 * nim zmienia — kliknięcia w tym repo zawołać się nie da, więc to jest najbliższa prawdzie
 * rzecz, jaką da się sprawdzić bez okna. Prawdziwym kliknięciem sądzi to chromium z `e2e/`.
 */
/* ZNAK, ZE WYBOR JEST ZYWY. Do 2026-09-01 niosl razem z nim `onChange` kontrolki `<select>`;
 * kontrolka jest dzis wlasna lista, wiec pick robi `onClick` pozycji, a to zostaje samym
 * znakiem. Kryterium pyta o niego, zeby kontrolka bez handlera nie przeszla jako kontrolka
 * (niezmiennik 16). */
export const CHOICE_IS_LIVE = { 'data-workflow-choice-live': 'yes' } as const;

interface WhichWorkflowProps {
  /** Wszystko, co ekran wyczytał z katalogu workflow — także pliki bez ani jednego kroku. */
  readonly choices: readonly Choice[];
  /** Nazwa pliku wskazana przez człowieka, albo `null`, kiedy nikt jeszcze nie wskazywał. */
  readonly chosen: string | null;
  /** Czy bieg idzie. Wtedy tej decyzji nie da się już zmienić i kontrolki nie ma. */
  readonly running: boolean;
  /** Workflow, który naprawdę ruszy — ta sama odpowiedź, którą ma nagłówek i przycisk Start. */
  readonly nextUp: Choice | null;
}

/**
 * Tyle, ile potrzebuje sama lista.
 *
 * `nextUp` bez `null`: pozycja zaznaczona jest tu WARUNKIEM istnienia kontrolki, a nie jej
 * polem — lista, która nie ma czego pokazać jako wybrane, nie ma prawa się narysować. Rozstrzyga
 * to [`choiceIn`] jednym warunkiem, więc ten komponent nie powtarza go u siebie.
 */
interface WorkflowChooserProps {
  readonly choices: readonly Choice[];
  readonly nextUp: Choice;
}

/** Tyle, ile potrzebuje zdanie o tym, kto wybrał: co leży w katalogu i co wskazał człowiek. */
interface WhoChoseItProps {
  readonly choices: readonly Choice[];
  readonly chosen: string | null;
}

/**
 * KTÓRY WORKFLOW RUSZY — wybór widoczny i zmienialny, W nagłówku, który go ogłasza.
 *
 * ── DLACZEGO TUTAJ, A NIE W PASKU LOADOUTU (zmierzone, nie ustalone gustem) ───────────────────
 *
 * Lista wyboru workflow stała w pasku do 2026-08-20 i oddała swoje miejsce liderowi. Wrócić tam
 * nie może z dwóch policzonych powodów. Pierwszy: rząd kontrolek paska jest pełny CO DO PIKSELA
 * przy oknie 1512 px — siedem kontrolek, a pole zadania zjechało do 128 px właśnie po to, żeby
 * nazwa sekcji i sufit wydatku zmieściły się w kadrze (`./start.tsx`, akapit przy polu zadania).
 * Ósma kontrolka wypycha stamtąd cudzą treść. Drugi: `docs/ARCHITECTURE.md` §7 daje 96 px chrome
 * nad pierwszą treścią, a widok domyślny wydaje 93 (8 + 1 + 32 + 52) — trzy piksele nie są
 * miejscem na kontrolkę, a nowy rząd paska kosztowałby ich kilkadziesiąt.
 *
 * Nagłówek biegu jest TREŚCIĄ, nie chrome, i cały ten rachunek stoi w `./strip/head.tsx`. Jest
 * też jedynym miejscem, w którym ekran już dziś ogłasza, co ruszy („READY TO RUN · Deep
 * research"), więc odpowiedź na to ogłoszenie należy dokładnie tam, gdzie ono padło.
 *
 * ── 2026-09-01: TYTUŁ NAGŁÓWKA *JEST* TĄ KONTROLKĄ ──────────────────────────────────────────
 *
 * ZGŁOSZENIE: nazwa workflow stała na tym ekranie TRZY RAZY — jako tytuł nagłówka, jako napis
 * na kontrolce startu („Run Murmur-1", `./start.tsx`) i jako zaznaczona pozycja tej listy,
 * o wiersz WYŻEJ od tytułu, który ją powtarzał. Jeden fakt, trzy nośniki (niezmiennik 13).
 * Trzy miejsca na jedną odpowiedź to trzy miejsca, w których da się jej zaprzeczyć — i tak
 * właśnie wyglądała wada, którą właściciel sfotografował: nagłówek ogłaszał jeden workflow,
 * przycisk obok nazywał drugi.
 *
 * KTÓRY NOŚNIK ZOSTAŁ, I DLACZEGO WŁAŚNIE TEN. Zostaje TYTUŁ, a kontrolka wyboru jest nim
 * — jednym elementem, nie dwoma zgodnymi. Rachunek jest taki:
 *
 *   NAPIS NA PRZYCISKU STARTU nie może być tym jedynym miejscem. Przycisk stoi w pasku
 *   loadoutu, czyli w chrome, i nie ma jak powiedzieć DWÓCH pozostałych rzeczy, których ta
 *   nazwa wymaga: że to był wybór i że da się go zmienić. Rzędu paska nie da się o to
 *   poszerzyć — jest pełny co do piksela przy oknie 1512 px, a `docs/ARCHITECTURE.md` §7
 *   zostawia 3 z 96 pikseli chrome (8 + 1 + 32 + 52 = 93). Przycisk mówi więc, CO ROBI
 *   naciśnięcie („Run workflow"), a nie co ono uruchomi.
 *
 *   TYTUŁ nie może zniknąć: nagłówek bez nazwy nie rysuje się wcale (`./strip/head.tsx`),
 *   więc zabrałby ze sobą stan biegu, metadaną i wydatek. Jest też jedynym miejscem, w którym
 *   ekran ogłasza, co jest gotowe — i jedynym stopniem pisma, w którym widać to z drugiego
 *   końca pokoju.
 *
 *   LISTA nie może pokazywać innego napisu niż zaznaczona pozycja — kontrolka wyboru, która
 *   nie pokazuje wybranego, jest zagadką. Dopóki tytuł i lista są dwoma elementami, ta sama
 *   nazwa stoi w obu.
 *
 * Zostaje więc jedno rozwiązanie, w którym nazwa stoi RAZ: tytuł i lista to ten sam element.
 * Człowiek czyta nazwę tam, gdzie ekran ją ogłasza, i zmienia ją tam, gdzie ją przeczytał —
 * bez wchodzenia w inną sekcję i bez szukania drugiej kontrolki. Zdanie „kto to wybrał" idzie
 * do wiersza nadoczka (`RunHeadProps.said`), bo odpowiada na inne pytanie i nie powtarza nazwy.
 *
 * GRANICA, POWIEDZIANA WPROST. Nazwa żyje jeszcze w PODPOWIEDZI kontrolki startu („Starts a new
 * run of the complete X workflow from the beginning", `./start.tsx`). To nie jest czwarty
 * nośnik na ekranie: podpowiedź jest niewidoczna, dopóki się na nią nie wskaże, liczona jest
 * z tego samego `Choice`, którego `path` ten przycisk wysyła — więc nie ma jak nazwać innego
 * workflow niż ten, który ruszy — i odpowiada na pytanie „co zrobi naciśnięcie", a nie „co jest
 * gotowe". Gdyby właściciel chciał ją zdjąć, zdanie o NOWYM biegu od początku (a nie o wznowieniu
 * rozmowy z liderem, zgłoszenie z 2026-08-21) musi przeżyć w innym miejscu.
 *
 * ── CZEGO TU NIE MA ──────────────────────────────────────────────────────────────────────────
 *
 * KONTROLKI W TRAKCIE BIEGU. Który workflow rusza, czyta się RAZ, przy starcie; kontrolka
 * przyjmująca tę decyzję w połowie biegu obiecywałaby zmianę, której nie da się wykonać
 * (niezmiennik 16) — ten sam powód, dla którego pole zadania i oba limity obok gasną. Tytuł
 * jest wtedy zwykłym napisem i nazywa bieg, który idzie.
 *
 * `<label>` WOKÓŁ ZDANIA. Tekst `<label>` staje się nazwą dostępną kontrolki, więc zdanie „kto
 * to wybrał" wpisane w `<label>` przemianowałoby wybór workflow na siebie. Nazwa jedzie przez
 * `aria-label` z `./choices.ts`, zdanie stoi wierszem wyżej jako zwykły napis.
 *
 * WŁASNEJ KRESKI I WŁASNEGO PASA: nagłówek ma je już (`px-[18px]` i `border-b`), a rząd, który
 * dokłada swoje wewnątrz cudzego, rysuje ramkę zamiast wiersza.
 */
function WhichWorkflow({ choices, nextUp }: WorkflowChooserProps): ReactElement {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);

  /* ZAMKNIJ, KIEDY CZLOWIEK PATRZY GDZIE INDZIEJ. Bez tego lista zostaje otwarta po kliknieciu
     w cokolwiek obok i zaslania kroki — a jest to jedyna rzecz na tym ekranie, ktora cokolwiek
     zaslania. Escape i klik poza pudelkiem robia to samo, bo to jedno zamkniecie. */
  useEffect(() => {
    if (!open) return undefined;
    const away = (event: globalThis.MouseEvent): void => {
      if (!(event.target instanceof Node) || box.current?.contains(event.target) !== true) {
        setOpen(false);
      }
    };
    const key = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', away);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('mousedown', away);
      document.removeEventListener('keydown', key);
    };
  }, [open]);

  return (
    /* WLASNA LISTA ZAMIAST `<select>`, i to nie jest ozdoba — zgloszenie wlasciciela 2026-09-01,
       powtorzone dwa razy: „czcionka totalnie za duza".

       Lista rozwijana `<select>` jest MENU SYSTEMU, nie naszym elementem. Dziedziczy stopien po
       kontrolce, a kontrolka niesie `text-title` (22 px), bo zamknieta JEST tytulem ekranu.
       Probowalem najtansza droga — `font-size` wprost na `<option>` — i macOS ja zignorowal
       (zmierzone na jego zrzucie po wdrozeniu). Zostaje wlasna lista: nasz element, nasz stopien.

       NAZWA DALEJ STOI RAZ na ekranie: przycisk pokazuje wybrana, a lista pozostale. Kiedy jest
       zamknieta, w drzewie nie ma jej wcale (`open ? ... : null`) — nie chowamy jej arkuszem,
       bo rzecz schowana arkuszem dalej czyta czytnik ekranu. */
    <span ref={box} className="relative inline-flex max-w-full items-center">
      <button
        type="button"
        data-workflow-choice
        {...CHOICE_IS_LIVE}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={WORKFLOW_LABEL}
        onClick={() => {
          setOpen((was) => !was);
        }}
        className="-ml-2 flex max-w-[560px] cursor-pointer items-center gap-2 rounded-md border-0 bg-transparent py-1 pr-2 pl-2 font-ui text-title text-ink outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent-edge"
      >
        <span className="truncate">{nextUp.name}</span>
        <svg
          aria-hidden="true"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="size-4 shrink-0 text-muted"
        >
          <path d="m4.4 6.4 3.6 3.6 3.6-3.6" />
        </svg>
      </button>

      {/* LISTA STOI W DRZEWIE ZAWSZE, schowana atrybutem `hidden`, a nie warunkiem w JSX.
          `hidden` to `display:none`, wiec czytnik ekranu tez ja pomija — nie jest to rzecz
          schowana arkuszem, ktora dalej sie oglasza. Zawsze w drzewie, bo „czy kontrolka
          oferuje to, co naprawde lezy w katalogu" jest faktem o kazdym pliku, a nie tylko
          o tym jednym, ktory akurat wybrano. */}
      <ul
        hidden={!open}
        data-workflow-choice-list
        role="listbox"
        aria-label={WORKFLOW_LABEL}
        className="pane absolute top-full left-0 z-20 mt-1 max-h-[320px] min-w-[260px] overflow-y-auto rounded-md p-1"
      >
        {choices.map((one) => (
          /* WSZYSTKO, CO LEZY W KATALOGU, takze pliki bez krokow — te jako niewybieralne,
               z powodem w napisie. Powod i granica stoja przy `offerFor` w `./choices.ts`. */
          <li key={one.path}>
            <button
              type="button"
              role="option"
              data-choice-path={one.path}
              aria-selected={one.path === nextUp.path}
              disabled={one.steps.length === 0}
              onClick={() => {
                pickWorkflow(one.path);
                setOpen(false);
              }}
              className="row w-full justify-start px-2 text-left text-ui aria-[selected=true]:text-accent"
            >
              {offerFor(one)}
            </button>
          </li>
        ))}
      </ul>
    </span>
  );
}

/**
 * KTO WYBRAŁ TEN WORKFLOW — jedno zdanie, trzy stany świata, i to jest cała odpowiedź na
 * zgłoszenie właściciela („czemu mi się ten deep reaserch pojawia, przecież nie wybrałem żadnego
 * workflow"). Treść liczy `./choices.ts`, bo to polityka, a nie wygląd.
 *
 * NIE POWTARZA NAZWY i nie ma prawa jej powtórzyć: nazwa stoi w tytule, wiersz niżej, i to jest
 * jedyne miejsce, w którym stoi (niezmiennik 13). To zdanie mówi wyłącznie, CZYJA to była
 * decyzja.
 */
function WhoChoseIt({ choices, chosen }: WhoChoseItProps): ReactElement {
  return (
    <p data-workflow-choice-said className="lead min-w-0 truncate">
      {whoChoseIt(choices, chosen)}
    </p>
  );
}

/**
 * Oba sloty nagłówka wypełnione RAZEM albo wcale — bo odpowiadają na jedno pytanie.
 *
 * JEDNO WYRAŻENIE, NIE DWA WARUNKI PRZY DWÓCH SLOTACH: „czy jest jeszcze o czym decydować"
 * rozstrzyga się w jednym miejscu, więc nie da się zostawić na ekranie zdania „Loadout picked
 * this one for you — change it here" nad tytułem, którego już nie da się zmienić.
 *
 * Kiedy nie ma czego uruchomić, nie ma też o czym decydować — a nagłówek nad tym i tak się nie
 * rysuje (`./strip/head.tsx`), więc lista wisiałaby pod niczym (DESIGN §6).
 */
function choiceIn({ choices, chosen, running, nextUp }: WhichWorkflowProps): {
  readonly chooser: ReactNode;
  readonly said: ReactNode;
} {
  if (running || nextUp === null) return { chooser: null, said: null };
  return {
    chooser: <WhichWorkflow choices={choices} nextUp={nextUp} />,
    said: <WhoChoseIt choices={choices} chosen={chosen} />,
  };
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

  /* Wybór jest stanem TEGO zamontowanego ekranu. Krótkie przekazanie przy odmontowaniu jest
   * potrzebne wyłącznie zielonemu Run w edytorze: tamten klik wraca do świeżej instancji tego
   * komponentu, a pending request zamraża wartość widoczną przed wyjściem z ekranu. */
  const [reflectionEnabled, setReflectionEnabled] = useState(reflectionForRequestedRun);
  const reflectionAtUnmount = useRef(reflectionEnabled);
  reflectionAtUnmount.current = reflectionEnabled;
  useEffect(() => {
    return () => {
      rememberReflectionChoice(reflectionAtUnmount.current);
    };
  }, []);

  /* Uchwyt do pola wiersza wejścia — po to, żeby kliknięcie w strumień mogło mu ODDAĆ kursor.
   * Powód w całości przy `caretBackToTheField`. */
  const field = useRef<HTMLInputElement>(null);

  /* Ta sama liczba, którą pokazuje kontrolka startu — jeden fakt, jedno miejsce (niezmiennik 13).
   * Gdyby ekran trzymał własną kopię, pasek kart mówiłby „of 3", kiedy suwak stoi na 8. */
  const atOnce = useSyncExternalStore(subscribeToAtOnce, atOnceNow, atOnceNow);
  /* SUFIT TEGO BIEGU, nie tego, który pojedzie następny — i to jest różnica, która powstała
   * 2026-08-29 razem z jednorazowym nadpisaniem z paska (`./limits/chosen`, `takeTheBudget`).
   * Chip nad liniami mówi „$3.41 of $20" o biegu, KTÓREGO TO SĄ LINIE; kwota następnego biegu
   * postawiona w tym mianowniku jest liczbą, której ten bieg nigdy nie dostał. */
  const budgetUsd = useSyncExternalStore(subscribeToBudget, budgetOfTheRun, budgetOfTheRun);

  /**
   * CO LEŻY W KATALOGU WORKFLOW — pozycje listy, nie same nazwy, i to jest zmiana z 2026-08-31.
   *
   * Do tego dnia ten ekran zapamiętywał wyłącznie NAZWY (`workflowNames`), bo tyle potrzebowała
   * podpowiedź pod `/run`. Pozycja niesie oprócz nazwy KROKI, ich POZYCJE i STRZAŁKI z pliku —
   * czyli dokładnie to, z czego rysuje się obraz planu. Bez nich obraz nie miał czego pokazać,
   * dopóki bieg nie ruszył, i sygnatura tego produktu była dla świeżego okna niewidzialna.
   */
  useEffect(() => {
    let alive = true;
    listWorkflows()
      .then((entries) => {
        if (alive) rememberWorkflows(toChoices(entries));
      })
      .catch((error: unknown) => {
        /* ZDANIE MÓWI TEN, KTO CZYTAŁ — od 2026-08-31, bo od tego dnia czyta tylko ten efekt.
         * Do tego dnia stało tu milczenie z dopiskiem „powie to kontrolka startu", a tamta
         * kontrolka miała własny odczyt tego samego katalogu; odkąd go nie ma, milczenie tutaj
         * byłoby ciszą po jedynej próbie. Wyjęcie zdania z odmowy mieszka w `src/ipc/why.ts`:
         * Tauri odrzuca NAPISEM, nie `Error`, więc warunek `instanceof Error` byłby tu zawsze
         * fałszywy. Do strumienia, tą samą drogą, co odmowa startu (`sayWhatDidNotStart`). */
        if (alive) sayWhatDidNotStart(why(error, 'Loadout could not read the workflows folder.'));
      });
    return () => {
      alive = false;
    };
  }, []);
  /* CO TEN EKRAN WIE Z DYSKU — jedna migawka, tą samą drogą, co pozostałe magazyny. Powód, dla
   * którego te trzy fakty mieszkają poza komponentem, stoi w całości w `./whats-ready.ts`. */
  const ready = useSyncExternalStore(subscribeToWhatIsReady, whatIsReady, whatIsReady);
  const choices = ready.choices;
  const lastRun = lastRunIn(ready, folder);
  const namesToRun = useMemo<readonly Named[]>(() => workflowNames(choices), [choices]);

  /* CO LEŻY W KATALOGACH NARZĘDZI AGENTOWYCH — nazwy, po których ukośnik w wierszu wejścia
   * przestaje być literówką (`./entry/entry.tsx`, `skillLine`).
   *
   * 2026-09-02 — TEN ODCZYT MUSIAŁ TU DOJŚĆ, i bez niego nowa gałąź nigdy się nie zapala.
   * `useSkills.load()` wołał do tego dnia WYŁĄCZNIE ekran Knowledge (`../knowledge/index.tsx`),
   * więc zbiór nazw był w tym ekranie pusty, a `/harbor-inventory` odbijało się od wiersza jako
   * nieznana komenda — mechanizm żywy przy martwej drodze (niezmiennik 29).
   *
   * PRZY ZMIANIE FOLDERU TAKŻE, bo lista odpowiada na pytanie „co widzi agent pracujący TUTAJ":
   * `list_skills` czyta półki projektu razem z globalnymi (`commands::skills::list_skills_in`),
   * więc przełączenie zakresu zmienia odpowiedź. Odmowa nie leci w górę i nie ma tu czego łapać —
   * obsługuje ją magazyn i zostawia w swoim stanie zdanie dla człowieka. */
  useEffect(() => {
    void useSkills.getState().load();
  }, [folder]);
  const installedSkills = useSyncExternalStore(
    useSkills.subscribe,
    useSkills.getState,
    useSkills.getState,
  ).installed;
  const skillNames = useMemo<readonly string[]>(
    () => installedSkills.map((one) => one.name),
    [installedSkills],
  );

  /**
   * KTÓRY WORKFLOW RUSZY, KIEDY CZŁOWIEK NACIŚNIE `Run` — ta sama funkcja nad tym samym
   * nośnikiem, o które pyta kontrolka startu (`./start.tsx`, `willRun(choices, ready.chosen)`).
   *
   * FUNKCJA, A NIE DRUGI WYBÓR. Do 2026-08-31 ten akapit obiecywał dokładnie to, a obietnica
   * była nieprawdziwa: obie strony wołały wprawdzie tę samą funkcję, ale każda nad WŁASNYM
   * odczytem katalogu — ten ekran nad `./whats-ready.ts`, kontrolka nad swoim `useState`.
   * Zmierzone w prawdziwym chromium: dwa niezależne `list_workflows` na jedno wejście w sekcję,
   * a w scenie z różnymi odpowiedziami nagłówek i przycisk nazywały dwa RÓŻNE pliki naraz,
   * po pełnym ustaniu okna. Jedno źródło znosi tę klasę rozjazdu (niezmiennik 13).
   */
  const nextUp = useMemo(() => willRun(choices, ready.chosen), [choices, ready.chosen]);

  const cards = useMemo(() => roster({ view, agents: factsOf(run.steps) }), [view, run.steps]);
  /**
   * CO RYSUJE PRAWA KOLUMNA. Jedno wyrażenie, bo jeden obraz: kroki, kto na nich stoi i co robi
   * teraz. Do 2026-08-31 te trzy fakty stały na ekranie w trzech miejscach naraz.
   *
   * ── OBRAZ STOI TAM, ZANIM COKOLWIEK RUSZY (2026-08-31, druga zmiana tego dnia) ────────────
   *
   * ZMIERZONE na zrzucie prawdziwego okna 1512×950 (`e2e/harness.ts`): graf biegu — sygnatura
   * całego produktu — pojawiał się WYŁĄCZNIE w trakcie biegu, bo jedynym jego źródłem był
   * `run.steps`, a ten jest pusty, dopóki `Start` nie wróci. Człowiek, który otworzył
   * aplikację z gotowym folderem, agentem i workflow, nie miał skąd wiedzieć, że ten produkt
   * w ogóle RYSUJE pracę: prawa kolumna była pustym prostokątem 268 px, a lewa — 1010 px czerni.
   *
   * KIEDY MAGAZYN BIEGU MILCZY, RYSUJEMY PLAN, KTÓRY RUSZY. Kroki, ich pozycje i strzałki
   * przyjeżdżają z PLIKU workflow (`./choices.ts`, `toChoices`), więc obraz jest legalny
   * (niezmiennik 17): ani jedna współrzędna i ani jedna strzałka nie jest wymyślona tutaj.
   * Wybór pliku nie jest wyborem tego ekranu — jest tą samą funkcją, którą pyta o niego
   * przycisk (`nextUp` wyżej), więc obraz i napis na przycisku mówią o jednym workflow.
   *
   * STAN KAŻDEGO KROKU TO `waiting`, bo taki jest: `planOf` stawia `pending`, a `agentStatusOf`
   * przekłada to na kafelek z obrysem, nie wypełniony. Blok wypełniony obiecuje, że krok się
   * udał [DESIGN §2] — a tutaj nic się jeszcze nie wydarzyło i obraz nie ma prawa mówić inaczej.
   *
   * PYTANIE I KARTY AGENTÓW NIE WCHODZĄ DO PODGLĄDU. Przypięte pytanie w tej chwili może
   * pochodzić wyłącznie od lidera — bieg nie idzie, więc żaden krok nie pyta — a przypisane do
   * kroku z pliku byłoby pytaniem postawionym przy kimś, kto go nie zadał (niezmiennik 17).
   * Karty są puste z tego samego powodu: kafelek agenta istnieje wtedy i tylko wtedy, gdy agent
   * coś nadał.
   *
   * MAGAZYN BIJE PLIK, ZAWSZE. Warunek pyta o `run.steps`, czyli o to, czy bieg ma kroki —
   * pierwszy krok wpisany przez `nowRunning` zdejmuje podgląd w tym samym renderze, w którym
   * wchodzi prawdziwy plan.
   */
  const plan = useMemo(() => {
    const live = planFor(run.steps, run.links, cards, view.now, view.pinned);
    if (live.steps.length > 0 || nextUp === null) return live;
    return planFor(nextUp.steps, nextUp.links ?? null, [], NOBODY_IS_WORKING, null);
  }, [run.steps, run.links, cards, view.now, view.pinned, nextUp]);
  /**
   * CO NAGŁÓWEK EKRANU MÓWI O TYM BIEGU — jedno wyrażenie, policzone bez okna
   * (`./strip/headline.ts`), bo to repo nie ma jsdom i wszystko, co da się rozstrzygnąć modelem,
   * ma być rozstrzygnięte modelem (niezmiennik 15).
   *
   * KROKI BIERZE STĄD, SKĄD OBRAZ: z magazynu biegu, a kiedy ten milczy — z pliku workflow, który
   * ruszy (`nextUp`). To jest ten sam warunek, co przy `plan` wyżej, i musi być ten sam: nagłówek
   * liczący kroki z samego magazynu mówiłby „Ready to run" nad czterema kafelkami, które widać,
   * i milczałby o ich liczbie (niezmiennik 13).
   *
   * `run.folder ?? folder` — bieg zna swój folder, bo okno samo go wysłało do `run_workflow`;
   * kiedy nic nie biegnie, workspace jest tym, na którym stoi karta. `folderName` skraca ścieżkę
   * do nazwy, czyli do tego samego napisu, który nosi karta — pełna ścieżka w metadanej nagłówka
   * wypchnęłaby resztę wiersza (DESIGN §1).
   */
  const headline = useMemo(
    () =>
      headlineFor({
        workflow: run.workflow,
        nextUp: nextUp?.name ?? '',
        steps: run.steps.length > 0 ? run.steps : (nextUp?.steps ?? []),
        lines: run.lines,
        droppedBefore: run.droppedBefore,
        workspace: (run.folder ?? folder) === null ? null : folderName(run.folder ?? folder ?? ''),
        agents: run.agents.length,
        budgetUsd,
      }),
    [
      run.workflow,
      run.steps,
      run.lines,
      run.droppedBefore,
      run.folder,
      run.agents,
      folder,
      nextUp,
      budgetUsd,
    ],
  );

  /* GDZIE STOI KARTA PYTANIA — jedno wyrażenie, policzone z TEGO SAMEGO planu, który obraz
   * rysuje. Dwa warunki, jeden po każdej stronie ekranu, rozjechałyby się przy pierwszym
   * pytaniu, którego nie da się przypisać do kroku: karta zniknęłaby wtedy z obu miejsc naraz,
   * a bieg stałby na niej do końca świata (niezmiennik 13). */
  const askedAtItsStep = useMemo(
    () => plan.steps.some((step) => step.asked !== undefined),
    [plan.steps],
  );
  /* KTÓRY KROK MA OTWARTĄ SZUFLADĘ. Magazyn na poziomie modułu (`./graph/opened.ts`) z tego
   * samego powodu, co przy ekranie agenta: wybór ma przeżyć wyjście do innej sekcji, a handler
   * zamknięty w komponencie byłby kodem, którego żadne kryterium nie umie dotknąć. */
  const openedStep = useSyncExternalStore(
    subscribeToOpenedStepStream,
    openedStepStream,
    openedStepStream,
  );
  const showingStep = plan.steps.find((step) => step.id === openedStep) ?? null;

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

  /* KTÓRY WORKFLOW RUSZY, W NAGŁÓWKU — nazwa jako tytuł i lista jako ten sam element, a pod nim
   * zdanie o tym, kto go wybrał. Oba sloty liczy jedno wyrażenie, powód przy [`choiceIn`]. */
  const choice = choiceIn({ choices, chosen: ready.chosen, running, nextUp });

  /* WEJŚCIE W TEGO, KTO ROBI TEN KROK — i to jest jedyna droga do ekranu jednego agenta, odkąd
   * prawa kolumna przestała być listą kafelków. Bez niej `openAgent`, cała `session/` i komenda
   * `rerun_step` byłyby mechanizmem z kompletem testów i zerem produkcyjnych wołających, czyli
   * tą samą wadą, którą to zadanie zdejmuje z ekranu (niezmiennik 16).
   *
   * FUNKCJA MODUŁOWA PO DRUGIEJ STRONIE (`session/open.ts`), a tutaj tylko tłumaczenie klucza
   * kroku na podpis agenta: to repo nie ma jsdom, więc handler zamknięty w komponencie byłby
   * kodem, którego żadne kryterium nie umie dotknąć.
   *
   * 2026-08-31 — KLIKNIĘCIE W KAFELEK OTWIERA TERAZ SZUFLADĘ POD OBRAZEM, nie ten ekran. Bieg
   * równoległy jest zwykłym biegiem (niezmiennik 11), więc pytanie „co robi ten jeden krok" nie
   * ma prawa kosztować widoku wszystkich pozostałych. Ekran agenta odpowiada na INNE pytanie —
   * co ten agent DOSTAŁ i co ZOSTAWIŁ, czyli na dwa bloki faktów z dysku, których w strumieniu
   * nie ma — i na to potrzebuje całego okna. Droga do niego prowadzi ze szuflady, jednym
   * przyciskiem: rzecz tania od razu, droga o jedno kliknięcie dalej. */
  const openTheWorker = (stepId: string): void => {
    openStepStream(stepId);
  };

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

  /* BIEG, KTÓRY IDZIE, KIEDY TO OKNO DOPIERO WSTAJE.
   *
   * # Po co to istnieje
   *
   * Zgłoszenie właściciela 2026-08-23: odmowa „A run is already going… Press Stop first", a pod
   * nią `/stop` → „Nothing is running." Zdanie ze Stopu naprawił Rust — on jeden wie, czy coś
   * idzie — ale samo pytanie „skąd okno ma to wiedzieć" zostało bez odpowiedzi. Pamięć okna
   * o żywym biegu jest ULOTNA: przeładowanie strony zeruje magazyny i moduł, a bieg po tamtej
   * stronie pracuje dalej. Człowiek widzi wtedy ekran bez paska i bez Stopu nad czymś, co
   * kosztuje pieniądze.
   *
   * # Skąd bierzemy odpowiedź
   *
   * Z historii tego zakresu, bez ani jednej nowej krawędzi: `list_runs` podaje `state`, a bieg
   * ze słowem `running` w SWOIM katalogu jest tym, który idzie. Biegi porzucone przez zamknięte
   * okno nie są tu pomyłką, bo sprzątanie przy starcie przepisuje je na `interrupted`, zanim
   * to okno cokolwiek zamówi (`ipc::AppState::settle_everything_left_behind`).
   *
   * # Czego to NIE robi i dlaczego
   *
   * Nie podaje kroków. Strumień linii należy do wywołania, które ten bieg zaczęło, i po
   * przeładowaniu nie da się do niego wrócić — pasek narysowany z migawki `run.json` stałby
   * w miejscu i wyglądałby jak bieg, który utknął. Pusta lista kroków jest tu tą samą decyzją,
   * co przy wznowieniu z historii (`io.ts`, `asARun`): lepiej nie rysować bloków, niż rysować
   * takie, które nie mówią prawdy (niezmiennik 17).
   *
   * Zdanie w strumieniu mówi to wprost, bo bez niego brak linii nad pracującym biegiem czyta się
   * jak bieg, który nic nie robi. */
  useEffect(() => {
    let alive = true;
    if (useRun.getState().workflow !== '') return undefined;
    listRuns(folder)
      .then(async (rows) => {
        if (!alive) return;
        /* HISTORIA TEGO FOLDERU WCHODZI DO PAMIĘCI EKRANU PRZY OKAZJI, i to jest ta sama
         * odpowiedź, nie drugi odczyt: karta ostatniego biegu w kolumnie strumienia i pytanie
         * „czy coś tu idzie" są dwoma pytaniami do JEDNEJ listy. Drugie `list_runs` przy tym
         * samym montażu dałoby dwie listy z dwóch chwil (niezmiennik 13). */
        rememberRuns(folder, rows);
        const going = theOneThatIsGoing(rows);
        if (going === null) return;
        if (runFor(folder).getState().workflow !== '') return;
        const opened = await readRun(folder, going.folder);
        if (!alive || runFor(folder).getState().workflow !== '') return;
        runFor(folder).getState().nowRunning(opened.title, [], folder, opened.workflowFile);
        showInStream(
          saidOf(
            `"${opened.title}" was already going when this window opened, so the lines from ` +
              'before are not here. Stop reaches it.',
          ),
        );
      })
      .catch(() => {
        /* Świadomie bez zdania na ekranie: nieczytelna historia mówi o sobie sama, kiedy
         * człowiek o nią poprosi (`/history`), a dwa zdania o jednym fakcie to dwa miejsca
         * prawdy. Okno bez tej odpowiedzi zachowuje się jak dotąd. */
      });
    return () => {
      alive = false;
    };
  }, [folder]);

  /**
   * Ilu agentów leży w bibliotece — wyłącznie po to, żeby przewodnik pierwszego uruchomienia
   * wiedział, czy krok „dodaj agenta" jest już zrobiony.
   *
   * ODCZYT PRZY KAŻDYM WEJŚCIU NA TEN EKRAN, a nie raz na życie okna, i to jest darmowe:
   * powłoka trzyma w drzewie DOKŁADNIE jedną sekcję (`src/App.tsx`), więc wyjście do Agents
   * i powrót odmontowuje ten ekran i montuje z powrotem. Bez tego człowiek, który właśnie
   * dodał pierwszego agenta, wracałby na przewodnik dalej mówiący mu, żeby go dodał.
   *
   * ODMOWA JEST CICHA, tak samo jak przy katalogu workflow niżej: o nieczytelnej bibliotece
   * mówi sekcja Agents, a dwa zdania o jednym fakcie to dwa miejsca prawdy (niezmiennik 13).
   * Skutek dla przewodnika jest wtedy najostrożniejszy z możliwych — krok zostaje niezrobiony,
   * czyli ekran dalej pokazuje drogę zamiast odhaczać coś, czego nie zobaczył.
   */
  useEffect(() => {
    let alive = true;
    listAgents()
      .then((saved) => {
        if (alive) rememberAgents(saved.length);
      })
      .catch(() => {
        /* Patrz akapit wyżej. */
      });
    return () => {
      alive = false;
    };
  }, []);

  /**
   * TRZY KROKI DO PIERWSZEGO BIEGU, policzone z tego, co naprawdę leży na dysku.
   *
   * Ten ekran jest jedynym miejscem w aplikacji, które widzi wszystkie trzy listy naraz —
   * zakresy, agentów i workflow — więc to on składa odpowiedź na pytanie „gdzie jestem na
   * drodze do pierwszego biegu". Sam przewodnik nie wie o dysku nic (`./first-run.tsx`),
   * a strumień nie wie nawet o nim (`./feed/feed.tsx`, props `guide`).
   *
   * ZNIKA W CAŁOŚCI, KIEDY NIE MA CZEGO POKAZAĆ: komplet ustawiony znaczy trzy odhaczone
   * wiersze zajmujące strefę pracy, czyli listę o niczym (niezmiennik 17). Wtedy strumień
   * wraca do swojego własnego zdania i ekran wygląda tak, jak wyglądał.
   */
  const setUpSoFar = useMemo(
    () =>
      firstRunSteps({
        workspaces: scopes.all.length,
        agents: ready.agents,
        /* Liczymy workflow, które DA SIĘ URUCHOMIĆ: `workflowNames` odsiewa pliki bez ani
         * jednego kroku, a workflow bez kroków nie jest krokiem zrobionym — Start odmówiłby
         * mu zdaniem „There are no steps yet.". */
        workflows: namesToRun.length,
      }),
    [scopes.all.length, ready.agents, namesToRun.length],
  );

  /* Rzeczy uruchomione komendą — jedna z czterech liczb, które decydują o układzie niżej. */
  const started = useSyncExternalStore(subscribeToStarted, startedThings, startedThings);

  /**
   * CZY POWITANIE JEST TYM EKRANEM — i wtedy dostaje całą taflę zamiast toru `1fr`.
   *
   * Odpowiedź liczy `./first-run.tsx` i tam stoi cały jej powód. Tutaj są wyłącznie cztery
   * liczby, po jednej z każdego miejsca, które umie coś do obszaru pracy włożyć: obraz planu,
   * rzeczy uruchomione komendą i dwie połowy strumienia. `view.history` i `view.now.rows` to
   * ten sam warunek, którym `./feed/feed.tsx` (`nothingYet`) wybiera między przewodnikiem
   * a wierszami — musi być ten sam, bo inaczej układ zdejmowałby kolumnę kroków w chwili,
   * w której strumień już rysuje bieg.
   */
  const welcomeIsTheScreen = welcomeIsTheWholeScreen(setUpSoFar, {
    steps: plan.steps.length,
    started: started.length,
    lines: view.history.length,
    live: view.now.rows.length,
  });

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
  /**
   * Człowiek odpowiedział na przypięte pytanie.
   *
   * DWIE DROGI, JEDNA KONTROLKA. W jednym strumieniu stoją dwa różne pytania: to od lidera, na
   * którym stoi ZABLOKOWANA TURA agenta, i to z kafelka kontrolnego, na którym stoi BIEG. Okno
   * nie ma jak ich rozróżnić — podaje więc podpis dalej i pyta Rusta, czy ktoś na to czekał.
   *
   * Model widoku dostaje odpowiedź ZAWSZE i pierwszy: to on zdejmuje przypięcie, a człowiek ma
   * zobaczyć skutek kliknięcia natychmiast, niezależnie od tego, którą drogą odpowiedź pojedzie.
   * Czekanie na IPC przed zdjęciem pytania byłoby przyciskiem, który przez chwilę nic nie robi.
   */
  function answerQuestion(questionId: number, option: string): void {
    const asked = view.pinned;
    runFeed.answer(questionId, option);
    if (asked === null || asked.id !== questionId) return;
    /* Odmowa jest tu porzucana z rozmysłem: `false` znaczy „nikt na to nie czekał", czyli
     * pytanie należało do biegu i idzie swoją dotychczasową drogą. Zdanie o tym na ekranie
     * mówiłoby człowiekowi o mechanice, której nie widzi i na którą nie ma wpływu. */
    void answerTheLead(onTop, folder, asked.agent, option).catch(() => undefined);
  }

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
   * Zdanie o biegu, którego nie udało się zacząć → DO STRUMIENIA, nie do slotu pod paskiem.
   *
   * 2026-08-22 (T-79) — CO TU BYŁO I DLACZEGO TO BYŁO ZA MAŁO. Stało tu `onSaid={setSaid}`, więc
   * odmowa startu lądowała w `useState` tego ekranu (`data-screen-said`). Stan renderu ginie
   * razem z komponentem, a `src/App.tsx` montuje dokładnie jedną sekcję: wyjście do Agentów
   * i powrót zostawiało bieg, który się nie zaczął, i ekran, który o tym milczy. Odmowa
   * o umiejętności, której krok nie mógł dostać, jest dokładnie tym zdaniem, którego nie wolno
   * zgubić — bez niego „agent nie zna tej umiejętności" wygląda z zewnątrz identycznie jak
   * „model nie uznał, że warto po nią sięgnąć" (niezmiennik 29).
   *
   * DO STRUMIENIA, A NIE DO OBU MIEJSC. Model widoku żyje na poziomie modułu (`./feed/live`), bo
   * bieg trwa dłużej niż ekran — więc wiersz przeżywa wyjście do innej sekcji, a `data-screen
   * -said` nie. Postawienie zdania w obu miejscach dałoby dwa żywe regiony na jeden fakt
   * (niezmiennik 13), z których jeden znika przy pierwszym przejściu między sekcjami. Slot pod
   * paskiem zostaje przy dwóch faktach, które o biegu nie mówią: przy folderze i przy Stopie.
   *
   * TĄ SAMĄ DROGĄ, CO ODPOWIEDZI WIERSZA WEJŚCIA (`./entry/entry.tsx` woła `onShowInStream
   * (saidOf(…))`): rozmowa z Loadoutem jest JEDNĄ historią, a nie dwiema połówkami w dwóch
   * miejscach ekranu.
   *
   * `null` znaczy „nie ma o czym mówić" i jest normalnym stanem — kontrolka startu czyści nim
   * poprzednią odpowiedź, zanim spróbuje jeszcze raz. Historii się nie czyści: wiersz, który
   * już stanął, opisuje to, co się naprawdę wydarzyło.
   */
  function sayWhatDidNotStart(sentence: string | null): void {
    if (sentence === null) return;
    showInStream(saidOf(sentence));
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

  /**
   * Zatrzymanie z wiersza wejścia. Oddaje to, co odpowiedział Rust: `false` znaczy „nie było
   * czego zatrzymać".
   *
   * WOŁANE ZAWSZE, także wtedy, gdy to okno nic o biegu nie wie — i to jest cała naprawa
   * zgłoszenia właściciela z 2026-08-23 („Nothing is running." nad biegiem, który pracował).
   * Pamięć okna o żywym biegu jest ulotna: gubi ją przeładowanie strony. Zapadka biegu jest
   * jedna na aplikację i mieszka po tamtej stronie, więc pytamy JĄ (niezmiennik 13).
   *
   * Błąd oddaje `true`: zdanie o nim stoi już na ekranie, a doklejenie do niego „Nothing is
   * running." byłoby drugą, sprzeczną odpowiedzią na tę samą próbę.
   */
  async function stopRun(): Promise<boolean> {
    setSaid(null);
    try {
      return await stop();
    } catch (error: unknown) {
      setSaid(why(error, 'Loadout could not stop the run.'));
      return true;
    }
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
       * W pasek loadoutu (`strip/strip.tsx`, stopien `.strip .title` z makiety). NAGLOWEK dalej
       * istnieje w drzewie — wymaga go `e2e/tests/sections-mount.spec.ts`, bo bez naglowka
       * „cos sie zamontowalo" nie odpowiada na pytanie, na ktorej sekcji stoisz — tylko nie
       * kosztuje juz osobnego rzedu.
       *
       * OD 2026-08-31 JEST TO `<h2>`, a `<h1>` tego ekranu nosi nazwe BIEGU (`strip/head.tsx`):
       * stopien pisma i poziom naglowka mowia wreszcie to samo, bo do tego dnia `h1` mierzyla
       * 15 px, a stojaca pod nia `h2` z nazwa biegu — 34 px. Sadzi to
       * `strip/the-eye-and-the-outline-agree.test.tsx`. Kryterium z przegladarki czyta
       * `main h1, main h2, … main h6`, wiec zejscie o jeden poziom go nie rusza — zmierzone
       * w chromium, 5 z 5 zielonych. */}
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
            heading={sectionEntry('run').label}
            /* KONTROLKI BIEGU STOJĄ W PASKU, w jego prawej grupie — powód (zmierzone 189 px
                 chrome przy sufcie 96) stoi przy `StripProps.controls`. Zdanie o tym, czego nie
                 udało się zacząć, wraca TUTAJ i ląduje w jedynym slocie tego ekranu: dwa
                 miejsca na „co powiedział Loadout" to dwa zdania sprzeczające się o to samo
                 (niezmiennik 13). */
            controls={
              /* `min-w-0` ZDJETE 2026-08-31 — patrz `./strip/strip.tsx`: rzad ma sie zatrzymac
                 na swojej tresci i dopiero WTEDY oddac sprawe przewijaniu paska. Bez tego
                 kurczyl sie ponizej niej, a kontrolki z `shrink-0` malowaly sie na sobie. */
              <div className="flex min-w-0 flex-1 items-center gap-2">
                {/* Diagnostyka należy do aktywnego workspace i stoi przy czynnościach tego
                    ekranu. W fazie `before` komponent jest pustym szkieletem: prawdziwy mount
                    istnieje, a kryterium pada na braku kontrolki, nie brakującym imporcie. */}
                <Diagnostics folder={folder} />
                {/* CZEGO TU JUŻ NIE MA: „Learn from this run". Zeszło 2026-09-01 do stopy
                    kolumny planu i cały rachunek — 1108 px danych wobec 1562 chcianych, z czego
                    454 px niedoboru to były DWA NAPISY tej jednej kontrolki — stoi przy
                    [`ReflectionToggle`] w `./reflection/toggle.tsx`. Krótko: zdanie „Left on, it
                    keeps…" miało w tym rzędzie ZERO pikseli szerokości, a rząd mieścił się
                    WYŁĄCZNIE dlatego, że je zjadł. */}
                <Start
                  running={running}
                  reflectionEnabled={reflectionEnabled}
                  onSaid={sayWhatDidNotStart}
                />
              </div>
            }
          />
          {/* CZEGO TU JUŻ NIE MA: samotnego przycisku „Add a workspace" nad strefą pracy.
                Stał tu od 2026-08-18 jako jedyna czynna kontrolka świeżego ekranu i naprawiał
                prawdziwy defekt — tyle że pytanie o folder jest PIERWSZYM z trzech kroków do
                pierwszego biegu, a nie osobnym zaproszeniem obok nich. Zszedł więc o jedno
                piętro niżej, do przewodnika w strefie pracy, razem ze swoim znacznikiem
                `data-add-workspace` i swoją czynnością (`openFolder`).

                DRUGI POWÓD JEST ZMIERZONY. `docs/ARCHITECTURE.md` §7 daje 96 px nad pierwszą
                treścią, a widok domyślny stoi na 93. Ten przycisk kosztował 44 px i stał NAD
                `[data-work]` — dlatego kolektor gęstości musi do dziś odmawiać pomiaru chrome,
                kiedy zaproszenie jest na ekranie (`scripts/density-collect.mjs`, `inviteIsUp`).
                W strefie pracy nie kosztuje ani piksela chrome. */}
          {/* Jeden pasek na BIEG (niezmiennik 13). Komponent sam znika, kiedy nie ma pauzy. */}
          <PausedBanner
            run={{
              waitingUntil: pausedUntil(run.lines),
              steps: run.steps.map((step) => step.state),
            }}
          />
        </div>

        <div
          data-work
          className="grid min-h-0"
          /* JEDNA TAFLA ALBO SIATKA PRACY, i rozstrzyga to `welcomeIsTheScreen` wyżej.
             Powitanie postawione w torze `1fr` tej siatki potrzebowało 1118 px, dostawało 802
             i przelewało nadmiar aż do `main` — cały powód stoi przy [`welcomeIsTheWholeScreen`],
             a co robi każdy z dwóch torów, stoi przy [`WHOLE_SURFACE`]. */
          style={
            welcomeIsTheScreen
              ? { gridTemplateColumns: WHOLE_SURFACE, gridTemplateRows: WHOLE_SURFACE }
              : { gridTemplateColumns: WORK_COLUMNS, gridTemplateRows: WORK_ROWS }
          }
        >
          {/* NAGŁÓWEK BIEGU — `.rhead` z makiety, na całą szerokość obszaru pracy. W siatce pracy
              pojemnik stoi zawsze, także pusty: rząd bierze się z KOLEJNOŚCI dzieci, więc pojemnik
              znikający z drzewa przesuwa obie kolumny o rząd wyżej (powód w całości przy
              [`WORK_ROWS`]). Sam nagłówek znika, kiedy nie ma czego nazwać (`./strip/head.tsx`).

              NA PIERWSZYM OTWARCIU NIE MA ANI POJEMNIKA, ANI NAGŁÓWKA, i nic się przez to nie
              przesuwa: ta siatka ma wtedy JEDEN rząd i jedno dziecko. Bieg, którego nie ma
              i którego nie ma z czego zacząć, nie ma nazwy. */}
          {welcomeIsTheScreen ? null : (
            <div style={{ gridColumn: '1 / -1' }}>
              <RunHead
                headline={headline}
                /* WYBÓR JEDZIE W TYTUŁ NAGŁÓWKA, a zdanie o tym, kto wybrał — wiersz wyżej.
                   Cały powód (nazwa stojąca na tym ekranie trzy razy) stoi przy
                   [`WhichWorkflow`]. Slotami, bo co leży w katalogu i kto to wybrał, wie TEN
                   ekran; nagłówek to rysuje. */
                chooser={choice.chooser}
                said={choice.said}
              />
            </div>
          )}

          {/* ŚCIEŻKA BIEGU — PIERWSZA kolumna widoku pracy, i to jest zmiana z 2026-08-31,
              nie przestawienie mebli. Ekran czyta się od lewej, a pierwsze pytanie człowieka,
              który patrzy na pracujących agentów, brzmi „na czym ten bieg stoi", nie „co przed
              chwilą ktoś powiedział". Miejsce i szerokość przyjeżdżają z reguły `.work`
              w makiecie i tam są sądzone (`./run-matches-mockup.test.tsx`).

              Kroki, strzałki i pozycje przyjeżdżają z pliku workflow, więc obraz jest legalny
              (reguła 17); kiedy ich nie ma — a plan jednego kroku, który okno składa dla
              wpisanego pytania, ich nie ma i mieć nie może — `RunGraph` MILCZY o kształcie
              i pokazuje kroki jako ŚCIEŻKĘ: znacznik przy każdym z nich i linia, która je łączy
              (`./graph/path.tsx`). Milczenie o kształcie nie jest milczeniem o pracy.

              GRUPA „STARTED" STOI NAD OBRAZEM, bo rzecz uruchomiona komendą nie jest agentem:
              nie ma kroku w planie i nie ma czego na tym obrazie narysować (`rail/processes.ts`).
              Znika razem z ostatnią rzeczą, która jeszcze biegnie.

              CAŁEJ TEJ KOLUMNY NIE MA NA PIERWSZYM OTWARCIU, i to jest naprawa, nie oszczędność
              miejsca: pusty pojemnik szeroki na 376 px zabierał je powitaniu, a powitanie miało
              wtedy 1118 px treści na 802 px toru. Wraca w tej samej klatce, w której wraca
              pierwszy krok, pierwszy kafelek `/start` albo pierwszy wiersz strumienia. */}
          {welcomeIsTheScreen ? null : (
            <div
              data-plan-column
              /* KOLUMNA W PIONIE, NIE SIATKA O STAŁEJ LICZBIE RZĘDÓW: grupa „Started" i nagłówek
               ZNIKAJĄ, kiedy nie mają czego pokazać, a siatka o trzech rzędach oddałaby wtedy
               obrazowi rząd `auto` zamiast reszty wysokości — czyli płótno wysokie na zero.
               `min-h-0` na obu piętrach, żeby przewijał się środek, a nie cała strona. */
              /* `border-r`, nie `border-l`: kolumna stoi od 2026-08-31 po LEWEJ, więc linia
               dzieląca ją od strumienia jest jej prawą krawędzią. Krawędź po niewłaściwej
               stronie rysuje ramkę wokół okna zamiast szwu między dwiema kolumnami. */
              className="glass flex min-h-0 min-w-0 flex-col overflow-hidden border-r border-line"
            >
              <StartedThings />
              {/* Nadoczko sekcji, wiec stopien `text-eyebrow` — on jeden nosi wersaliki (DESIGN §4).
                ZERO KROKÓW TO ZERO NAGŁÓWKA (niezmiennik 17, DESIGN §6): nagłówek nad pustką
                obiecuje listę, na którą nic nigdy nie wejdzie. */}
              {/* 2026-08-31 — WARUNEK PYTA O `plan`, NIE O `run.steps`. Obraz rysuje od tego dnia
                także plan, który dopiero ruszy (patrz akapit przy [`plan`]), a nagłówek
                policzony z magazynu biegu znikałby wtedy nad pełnym płótnem — czyli mówiłby
                „nic tu nie ma" o czterech kafelkach, które widać. */}
              {plan.steps.length === 0 ? null : (
                <h2 className="shrink-0 px-[14px] pt-3 pb-[9px] font-mono text-eyebrow text-muted">
                  Steps
                </h2>
              )}
              <div className="grid min-h-0 flex-1">
                {/* ODPOWIEDŹ JEDZIE TĄ SAMĄ FUNKCJĄ, CO Z DOŁU STRUMIENIA — jeden tor, dwa
                  możliwe miejsca karty. Druga droga do odblokowania biegu byłaby drugim
                  miejscem, z którego da się go puścić (niezmiennik 13). */}
                {/* CO SIĘ STANIE, KIEDY OSTATNI KROK ZZIELENIEJE — POD ostatnim krokiem, bo
                  odpowiada na pytanie, które rodzi się dopiero po przeczytaniu ścieżki. Jedzie
                  propsem, bo gdzie kończą się kroki, wie rysunek, a nie ten ekran: postawione
                  tutaj, pod obszarem wysokim na całą kolumnę, stało od ostatniego kroku o pół
                  ekranu pustki. Jedno miejsce w drzewie, jeden mount (niezmiennik 13). */}
                <RunGraph
                  plan={plan}
                  onOpen={openTheWorker}
                  onAnswer={answerQuestion}
                  footer={<AfterRun plan={plan} />}
                />
              </div>
              {/* SZUFLADA POD OBRAZEM — to, co powiedział jeden krok, bez zdejmowania z oczu
                pozostałych. Wchodzi po kliknięciu w kafelek i schodzi na Escape albo na
                `Close`; obie drogi wołają tę samą funkcję (`./graph/opened.ts`).

                ZOSTAJE PO ZEJŚCIU BIEGU, i to jest decyzja, nie przeoczenie: to jest transkrypt,
                czyli zapis tego, co się stało, a nie kontrolka opisująca żywy bieg. Transkrypt
                biegu, który właśnie zszedł, jest jedyną rzeczą, po którą człowiek na ten ekran
                wraca (`./feed/model.ts`, akapit przy `runEnded`). */}
              {showingStep === null ? null : (
                <StepStream
                  step={showingStep}
                  view={view}
                  onToggle={runFeed.toggle}
                  onClose={closeStepStream}
                  {...(showingStep.who === undefined
                    ? {}
                    : {
                        onOpenAgent: () => {
                          openAgent(showingStep.name);
                        },
                      })}
                />
              )}

              {/* CZEGO LOADOUT NAUCZY SIĘ Z TEGO BIEGU — u stopy kolumny, poza wycinkiem, który
                  przewijają kroki. Dlaczego to zeszło z paska loadoutu i dlaczego wylądowało
                  DOKŁADNIE tutaj, stoi w całości przy [`ReflectionToggle`]
                  (`./reflection/toggle.tsx`) — z pomiarem, którego to przeniesienie jest
                  wnioskiem. Krótko: to jest fakt o KOŃCU biegu, jak karta obok („when the last
                  step turns green"), a zdanie, które go tłumaczy, mieści się tu w całości, bo
                  kolumna ma 376 px i wolno mu się zawinąć.

                  POZA `tail`em ŚCIEŻKI, choć tamta karta jedzie właśnie nim: karta jest zdaniem
                  o ostatnim kroku i ma stać pod nim, a to jest KONTROLKA ustawiana przed
                  startem. W planie o trzydziestu dwóch krokach ogon ścieżki leży poniżej kadru
                  i dałoby się do niej dojść wyłącznie przewinięciem (niezmiennik 16). */}
              <ReflectionToggle
                enabled={reflectionEnabled}
                disabled={running}
                onChange={setReflectionEnabled}
              />
            </div>
          )}

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
              onAnswer={answerQuestion}
              onJumpToNewest={runFeed.jumpToNewest}
              /* PYTANIE MA JEDNO ŻYWE MIEJSCE. Kiedy da się powiedzieć, KTÓRY krok pyta, karta
                 stoi pod nim, na obrazie; kiedy się nie da — pyta lider albo pod-agent
                 rozpuszczony w biegu — zostaje tutaj, gdzie stała od zawsze. Rozstrzyga to
                 jedno wyrażenie wyżej, policzone z tego samego planu, z którego rysuje się
                 obraz (niezmiennik 13). */
              askedAtItsStep={askedAtItsStep}
              /* PRZEWODNIK STOI W STREFIE PRACY, czyli tam, gdzie po pierwszym uruchomieniu
                 patrzy oko — i tylko dopóki cokolwiek zostało do zrobienia. Strumień wybiera
                 CHWILĘ (historia jest pusta), ekran wybiera TREŚĆ; żadne z nich nie zna
                 połowy drugiego. */
              {...(somethingIsLeft(setUpSoFar) || !running
                ? {
                    guide: somethingIsLeft(setUpSoFar) ? (
                      <FirstRun steps={setUpSoFar} onAddWorkspace={openFolder} />
                    ) : (
                      /* KOMPLET USTAWIONY — I TO NADAL NIE JEST POWÓD, ŻEBY MÓWIĆ „nothing here".
                       *
                       * Do 2026-08-31 ekran oddawał wtedy strumieniowi jego własne zdanie o braku
                       * wierszy, czyli komunikat o braku danych, którego DESIGN §6 zabrania wprost.
                       * Odpowiedź, która tu należy, jest o dwa piętra konkretniejsza i cała stoi
                       * w `./ready.tsx`: ostatni bieg tego folderu, a kiedy nie było żadnego —
                       * jedno zdanie w trybie rozkazującym, nazywające kontrolkę, która go zacznie.
                       *
                       * WEJŚCIE JEDZIE TĄ SAMĄ FUNKCJĄ, CO WIERSZ PANELU HISTORII (`openOneRun`),
                       * więc do zapisanego biegu prowadzi jedna droga, nie dwie (niezmiennik 13).
                       * Odmowa lądowania w panelu jest po tamtej stronie i mówi tam o sobie sama. */
                      <ReadyToRun
                        lastRun={lastRun}
                        onOpenLastRun={() => {
                          void openOneRun(folder, lastRun?.folder ?? '');
                        }}
                      />
                    ),
                  }
                : /* BIEG IDZIE, A LINII JESZCZE NIE MA — i wtedy ekran NIE opowiada o biegu
                     poprzednim. Karta „Last run" postawiona nad biegiem, który właśnie ruszył,
                     jest zdaniem o przeszłości w miejscu, w które człowiek patrzy na
                     teraźniejszość; obraz obok pokazuje już kroki TEGO biegu. Strumień mówi
                     wtedy swoje własne zdanie o wierszach, których nie ma — i to jest dokładnie
                     tyle, ile w tej chwili wiadomo. */
                  {})}
            />
            {/* ODPOWIEDŹ TEGO EKRANU — o folderze i o zatrzymaniu wywołanym z wiersza wejścia.
                Stoi NAD wierszem wejścia, bo dotyczy tego, co człowiek właśnie w nim napisał;
                do 2026-08-31 stała pod paskiem loadoutu, czyli po drugiej stronie ekranu od
                miejsca, w które patrzy oko po naciśnięciu Enter. Osobny znacznik od `data-said`
                w `start.tsx`: tamten mówi o kontrolce startu, ten o kartach, więc to są dwa
                różne fakty, a nie dwa miejsca na jeden (niezmiennik 13). Cicha porażka wygląda
                dokładnie jak martwa kontrolka.

                `fade-in`: zdanie POJAWIA SIĘ po czynności, która je wywołała, a wiersz wchodzący
                skokiem czyta się jak wiersz, który stał tam od początku — czyli jak coś, co
                człowiek przegapił. Bez przesunięcia i bez sprężyny: to jest tekst do
                przeczytania, nie powierzchnia, która wjeżdża (DESIGN §7). */}
            {said === null ? null : (
              <p data-screen-said className="lead fade-in px-[18px] pb-1" data-tone="fail">
                {said}
              </p>
            )}
            <Entry
              /* SZKIC NALEŻY DO ROZMOWY, nie do całego ekranu. Zmiana folderu albo terminalu
                 odmontowuje jego właściciela: cleanup odcina blob URL, a nowe pole zaczyna
                 puste. To świadomy kontrakt „clear on switch" — mniejszy koszt niż screenshot
                 z projektu A wysłany przez Enter w projekcie B. Fallback terminalu i wyjątek
                 zamknięcia są składane raz, wyżej, w `entryKey`. */
              key={entryKey}
              onOpenFolder={openFolder}
              /* BEZ WARUNKU `running`. Do 2026-08-23 stało tu `running ? stopRun : null`, czyli
                 wiersz odpowiadał z pamięci okna — a ta pamięć bywa nieprawdziwa i wtedy `/stop`
                 mówiło „Nothing is running." nad pracującym biegiem. Odpowiada Rust. */
              onStopRun={stopRun}
              onSayToAgent={sayIt}
              /* `/run` idzie WPROST do polityki startu, bez przechodzenia przez ten komponent:
                 `startFromLine` czyta katalog workflow, rozbiera linię i woła `launchRun` z tym
                 samym limitem, który trzyma suwak obok Startu. Zdanie odmowy wraca do wiersza,
                 bo dotyczy tego, co człowiek właśnie napisał. */
              onRunWorkflow={(rest) => startFromLine(rest, reflectionEnabled)}
              /* Kto pracuje, żeby wiersz mógł powiedzieć POD polem, gdzie pójdzie zdanie —
                 zamiast pozwolić człowiekowi wysłać je w ciemno. */
              talkingTo={listening}
              workflows={namesToRun}
              /* NAZWY UMIEJĘTNOŚCI, po których ukośnik przestaje być literówką i jedzie do lidera
                 znak w znak. Powód, dla którego odczyt mieszka w tym ekranie, stoi przy
                 `skillNames`. */
              skills={skillNames}
              /* ŚLAD PO KAŻDEJ WYSŁANEJ LINII. Wiersz składa `entry/echo.ts`, a ten ekran wie
                 tylko, DO KTÓREJ sesji strumienia on należy — powód przy `showInStream`. */
              onShowInStream={showInStream}
              /* Uchwyt do pola, żeby kliknięcie w tę kolumnę mogło oddać mu kursor. */
              fieldRef={field}
            />
          </div>
        </div>
      </div>

      {/* HISTORIA BIEGÓW TEGO FOLDERU — panel, który stawia `/history` z wiersza wejścia.
          Sam znika, kiedy nikt o nią nie poprosił, więc ekran pracy nie ma tu ani jednej
          gałęzi do podjęcia. Montowany TUTAJ, obok pytania o zamknięcie karty, bo obie te
          rzeczy stoją NAD widokiem pracy i obie stawia magazyn na poziomie modułu — a wtedy
          jedno miejsce w drzewie odpowiada za wszystko, co ten ekran zakrywa.

          Bez tej jednej linii `/history` byłoby komendą, która czyta dysk i nie ma jak niczego
          pokazać: mechanizm bez wołającego jest wadą, nie postępem (niezmiennik 16). */}
      <PastRuns />

      {/* EKRAN JEDNEGO AGENTA. Rysuje się WYŁĄCZNIE wtedy, gdy któryś jest otwarty, i stoi tu —
          obok historii biegów, nie w kolumnie planu — bo zakrywa całe okno, a nie kolumnę.
          Do 2026-08-31 montowała go lista agentów; razem z nią zniknęłoby jedyne miejsce, w
          którym ten ekran w ogóle powstaje.

          `onSaid` jest całą naprawą „Run this step again": ekran agenta rysuje ten przycisk
          wyłącznie wtedy, gdy ma dokąd oddać odpowiedź. Cała droga pod spodem (`rerun_step`,
          `./io.ts`, `./rail/again.ts`, przycisk w `./session/session.tsx`) miała wołających
          wyłącznie w testach: mechanizm działa, kiedy go zawołać, i nikt go nie wołał
          (niezmiennik 29). */}
      <AgentScreen cards={cards} onSaid={sayAfterRunningAgain} />

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
