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
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import type { ReactElement } from 'react';
/* OKNO WYBORU FOLDERU JEST WTYCZKĄ TAURI, nie komendą Loadouta — `dialog:allow-open` stoi
 * w `src-tauri/capabilities/default.json` od T-01 i do dziś nie miało ANI JEDNEGO wołającego,
 * czyli było uprawnieniem, którego nikt nie używa, a takie uprawnienie jest dziurą, o której
 * nikt się nie dowie (komentarz w tamtym pliku mówi to wprost). Import stoi TU, a nie
 * w `io.ts`, bo tamten plik należy dziś do równoległego zadania (AGENTS.md §7); kiedy się
 * zwolni, ta jedna linia przenosi się tam razem z `openFolder`, żeby granica sekcji miała
 * jedno miejsce (niezmiennik 23). */
import { open as chooseFolder } from '@tauri-apps/plugin-dialog';

import type { FeedLine, Step } from '../../state/run';
import { useRun } from '../../state/run';
import type { WorkspacesStore } from '../../state/workspaces';
import { createWorkspacesStore } from '../../state/workspaces';
import { Feed } from './feed/feed';
import { attachPort, runFeed } from './feed/live';
import type { FeedView } from './feed/model';
import { Now } from './feed/now';
import { Entry } from './entry/entry';
import { stop } from './io';
import { PausedBanner } from './limits/paused-banner';
import type { AgentFacts } from './rail/roster';
import { roster } from './rail/roster';
import { Rail, RAIL_WIDTH } from './rail/rail';
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

/* Ta sama migawka dla okna i dla renderu serwerowego. Model nie ma stanu „po stronie serwera":
 * `renderToStaticMarkup` widzi po prostu bieg, którego jeszcze nie ma. */
function currentView(): FeedView {
  return runFeed.view;
}

/**
 * Karty tego okna — magazyn na poziomie modułu, jak `runFeed`.
 *
 * ZATRZYMANIE WCHODZI ARGUMENTEM i dziś jest nim `stop()` z `io.ts`, czyli `stop_run`. To jest
 * poprawne dopóty, dopóki okno prowadzi JEDEN bieg: identyfikatora karty ta komenda nie bierze,
 * bo po tamtej stronie granicy nie ma dziś czego nim wybrać. Dzień, w którym biegów będzie
 * więcej niż jeden, jest dniem, w którym `stop_run` dostaje argument — i to jest ta jedna
 * linia, która się wtedy zmienia.
 *
 * Wyeksportowany, bo test kryterium musi móc zasiać karty i zobaczyć, że przełączenie naprawdę
 * przestawiło magazyn, a nie tylko klasę na przycisku.
 */
export const workspaces: WorkspacesStore = createWorkspacesStore(() => stop());

/** Nazwa folderu, czyli to, co karta mówi o sobie na pasku. Pełna ścieżka zostaje w podpowiedzi. */
function folderName(path: string): string {
  return (
    path
      .split('/')
      .filter((part) => part !== '')
      .at(-1) ?? path
  );
}

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

/** Zdanie odmowy, które napisał Rust; własne dokładamy tylko wtedy, gdy jego nie ma. */
function why(error: unknown, mine: string): string {
  const said = error instanceof Error ? error.message.trim() : '';
  return said === '' ? mine : said;
}

export default function Run(): ReactElement {
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);
  const run = useSyncExternalStore(useRun.subscribe, useRun.getState, useRun.getState);
  const tabs = useSyncExternalStore(workspaces.subscribe, workspaces.getState, workspaces.getState);

  /* Jedno miejsce na to, co Loadout odpowiedział o folderze albo o zatrzymaniu wywołanym
   * z wiersza wejścia. Cicha porażka wygląda dokładnie jak martwa kontrolka. */
  const [said, setSaid] = useState<string | null>(null);

  const strip = useMemo(() => stripFor(run.workflow, run.steps), [run.workflow, run.steps]);
  const cards = useMemo(() => roster({ view, agents: factsOf(run.steps) }), [view, run.steps]);

  /* LICZBA ZYWYCH AGENTOW WRACA DO KARTY, i to nie jest ozdoba.
   *
   * `WorkspaceTab.agents` bylo pisane tylko przy zakladaniu karty i zawsze zerem, wiec
   * `requestClose` zawsze wchodzil w galaz „nic tu nie chodzi": karta z ZYWYM biegiem znikala
   * bez pytania i bez `cancel(id)`, czyli bieg zostawal osierocony i dalej pali limit
   * (niezmiennik 6 nazywa to bledem finansowym, nie higienicznym). Potwierdzenie zamkniecia
   * — zamontowane i przetestowane — bylo przez to kodem NIEOSIAGALNYM.
   *
   * Zrodlem jest ta sama lista, ktora rysuje szyne, wiec liczba na karcie i kafelki obok siebie
   * nie moga sie rozjechac (niezmiennik 13). Tylko karta na wierzchu: silnik prowadzi jeden bieg
   * i nie mowi, czyj on jest, wiec kazda inna karta dostalaby zgadniete zero z kropka „tu cos
   * chodzi" nad folderem, w ktorym nic nie chodzi (niezmiennik 17). */
  useEffect(() => {
    const active = tabs.activeId;
    if (active === null) return;
    workspaces.getState().setAgents(active, cards.length);
  }, [tabs.activeId, cards.length]);
  const running = run.workflow !== '';

  /** Wybór folderu — ta sama czynność pod `＋` na pasku kart i pod `/open` w wierszu wejścia. */
  function openFolder(): void {
    setSaid(null);
    chooseFolder({ directory: true, multiple: false, title: 'Choose a folder to work in' })
      .then((picked) => {
        /* Anulowanie okna wyboru jest wartością, nie błędem (niezmiennik 7): człowiek się
         * rozmyślił i nie ma o czym mówić. */
        if (typeof picked !== 'string') return;
        workspaces.getState().open({
          /* Ścieżka JEST identyfikatorem karty: jeden folder = jedna karta (§6a reguła 1),
           * a kanoniczną ścieżkę oddaje okno wyboru systemu. */
          id: picked,
          name: folderName(picked),
          path: picked,
          /* Zero, bo w folderze dopiero co otwartym nikt nie pracuje. Liczba żywych agentów
           * per folder nie ma dziś źródła — silnik prowadzi jeden bieg i nie mówi, czyj on
           * jest — a wpisanie tu czegokolwiek innego byłoby kropką „tu coś chodzi" nad
           * folderem, w którym nic nie chodzi (niezmiennik 17). */
          agents: 0,
        });
      })
      .catch((error: unknown) => {
        setSaid(why(error, 'Loadout could not open the folder chooser.'));
      });
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
      {/* Ten sam pasek, co w Agents, Skills, Memory i Workflows — jedna konwencja na pięć
          sekcji, a nie pięć wariantów tej samej odpowiedzi na pytanie „gdzie jestem". */}
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Run</h1>
      </header>

      <div className="grid min-h-0 flex-1" style={{ gridTemplateRows: SCREEN_ROWS }}>
        <TabBar
          tabs={tabs.tabs}
          activeId={tabs.activeId}
          /* Nikt nie czeka w kolejce, więc pasek nie rysuje zdania o miejscach i te dwie
           * liczby nigdzie nie docierają. Pula jest jedna na całą aplikację (niezmiennik 11)
           * i dziś nie ma w oknie nikogo, kto by ją znał; kiedy limiter będzie, przyjadą tu
           * wszystkie trzy naraz — a nie dwie z nich zgadnięte. */
          busy={0}
          atOnce={0}
          waitingIn={null}
          onSelect={tabs.activate}
          onClose={tabs.requestClose}
          onOpenFolder={openFolder}
        />

        <div className="flex shrink-0 flex-col gap-2">
          <div className="flex items-center gap-3">
            <Strip strip={strip} />
            {/* 2026-08-18: Start stoi tu, a nie w makiecie — makieta zaczyna bieg wierszem
             * wejścia, czyli parserem, którego to repo świadomie nie ma (`start.tsx`). Dopóki
             * wybór workflow i limit „ile naraz" mieszkają w tej kontrolce, ona jest jedynym
             * miejscem, z którego da się zacząć bieg. */}
            <Start running={running} />
          </div>

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
            <Now now={view.now} />
            <Entry onOpenFolder={openFolder} onStopRun={running ? stopRun : null} />
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
