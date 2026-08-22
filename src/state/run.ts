/* Magazyn biegu: bufor linii i nic poza tym.
 *
 * Ten plik ma jedną robotę — DOPISAĆ i UCIĄĆ. Żadnego sklejania, żadnego zwijania, żadnych
 * etykiet: to wszystko mieszka w `src/sections/run/feed/model.ts`, bo tam da się je
 * przetestować bez okna, a tutaj rosłoby jako drugie miejsce prawdy o tym samym (niezmiennik 23).
 *
 * Limit 2000 linii wolno mieć TYLKO dlatego, że reszta biegu leży w `logs/agent-<id>.jsonl`
 * i w SQLite — pamięć jest oknem, pliki są prawdą (niezmiennik 4). Okno bez licznika tego,
 * co z niego wypadło, cicho kłamie: „Load earlier" nie ma o co poprosić, więc albo pobiera
 * od zera, albo nic. Stąd DWA pola obok siebie, i każde odpowiada na inne pytanie:
 *   `droppedBefore`    ile linii już wypadło — czy „Load earlier" ma w ogóle po co istnieć
 *                      (niezmiennik 16: kontrolka bez roboty nie wchodzi do repo),
 *   `earliestKnownId`  identyfikator najstarszej linii, którą jeszcze mamy — czyli granica,
 *                      od której ta kontrolka prosi o stronę wstecz.
 *
 * Typy `FeedLine` i `ForeignLine` stoją TUTAJ, a nie w sekcji, żeby zależność szła w jedną
 * stronę: sekcja zna magazyn, magazyn nie zna sekcji.
 *
 * 2026-08-18 — SESJI JEST TYLE, ILE ZAKRESÓW. Ten plik oddawał do dziś jeden magazyn na całą
 * aplikację, a od dziś zakres pracy wybiera się w bocznym menu i przełączenie go **nie ma
 * prawa zgubić biegu**. Rejestr, uchwyt do sesji aktywnego zakresu i powód, dla którego jedno
 * bez drugiego nie wystarcza, stoją na końcu pliku.
 */
import { create, useStore } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';
import type { Line } from '../ipc/types';
/* ZAKRES PRACY MA JEDNO ŹRÓDŁO (niezmiennik 13) i jest nim ten magazyn — `activeId` jest
 * kluczem sesji. Import idzie w tę stronę i nigdy w drugą: magazyn zakresów nie wie, że
 * istnieją biegi, a gdyby wiedział, przełączenie zakresu musiałoby coś o biegu decydować. */
import { useWorkspaces } from './workspaces';

/** Dwa pola, które granica dokłada wierszowi z drutu. */
export interface Stamped {
  /** Ściśle rosnący numer nadawany po stronie Rusta [T2 §6.3]. */
  readonly id: number;
  /** Kiedy zdarzenie napłynęło, w milisekundach. Okno sklejania liczy się z tego. */
  readonly at: number;
}

/** Wiersz, który to repo umie nazwać: jeden z czternastu rodzajów, ostemplowany. */
export type FeedLine = Line & Stamped;

/**
 * Wiersz, którego rodzaju to repo NIE zna.
 *
 * Nie jest to hipoteza: vendorzy dokładają typy zdarzeń co tydzień i po cichu, a lustro
 * `src/ipc/types.ts` jest pisane ręcznie. Kształt jest w typie, żeby model musiał się
 * z takim wierszem zmierzyć w czasie kompilacji, a nie dopiero na ekranie użytkownika
 * (niezmiennik 5 w duchu, po stronie frontu).
 */
export interface ForeignLine extends Stamped {
  readonly kind: string;
  readonly agent: string;
}

/** Cokolwiek, co może wjechać kanałem. */
export type Incoming = FeedLine | ForeignLine;

/** Siedem stanów kroku [ARCHITECTURE §5]. `paused` jest stanem BIEGU, nigdy kroku. */
export type StepState =
  'pending' | 'ready' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'skipped';

/** Krok biegu w kolejności grafu — jeden do jednego z blokiem paska loadoutu. */
export interface Step {
  readonly id: string;
  readonly name: string;
  readonly state: StepState;
}

/** Kto to powiedział — trzy wartości, nie osiem [00-SYNTHESIS §2.2]. */
export type Who = 'you' | 'agent' | 'loadout';

/** Odpowiedź człowieka na pytanie agenta. */
export interface Answer {
  readonly questionId: number;
  readonly option: string;
  readonly who: Who;
}

/** Ile linii biegu trzymamy w pamięci naraz [T2 §6.3, obrona 5]. */
export const LINE_LIMIT = 2000;

export interface RunState {
  /** Okno ostatnich `LINE_LIMIT` linii, najstarsza pierwsza. */
  readonly lines: readonly FeedLine[];
  /** Ile linii wypadło z głowy okna od początku biegu. */
  readonly droppedBefore: number;
  /** Identyfikator najstarszej linii, którą jeszcze mamy; `null`, dopóki nie ma żadnej. */
  readonly earliestKnownId: number | null;
  /** Agenci, którzy w tym biegu wystąpili, w kolejności pojawienia się. */
  readonly agents: readonly string[];
  /**
   * Kroki biegu w kolejności grafu.
   *
   * Przychodzą z grafu razem ze startem biegu, nie z linii — linia `step` jest kreską
   * w strumieniu, a nie stanem kroku, więc odtwarzanie z niej listy kroków dałoby pasek,
   * który rośnie w trakcie biegu zamiast pokazywać plan od pierwszej sekundy (niezmiennik 17).
   *
   * 2026-08-18 — OBIETNICA Z TEGO KOMENTARZA JEST WRESZCIE DOTRZYMANA. Do dziś stało tu
   * „wypełnia je komenda startu biegu", a nic w repo tego nie robiło: pole nie miało settera
   * i nie miało pisarza. Pisze je teraz `nowRunning`, wołane z jednego miejsca —
   * `src/sections/run/io.ts`, czyli z tej samej ścieżki Startu, która woła `run_workflow`.
   */
  readonly steps: readonly Step[];
  /**
   * Nazwa workflow, który ten bieg wykonuje — pierwszy człon podpisu paska loadoutu.
   *
   * Stoi obok `steps`, bo przychodzi tą samą drogą i w tej samej chwili: pasek bez nazwy
   * i pasek bez kroków to ten sam brak. Puste, dopóki nie ma biegu — i to puste jest zarazem
   * jedyną odpowiedzią całej aplikacji na pytanie „czy coś biegnie" (niezmiennik 13), z której
   * żyje przycisk Stop.
   */
  readonly workflow: string;
  /**
   * Nazwa PLIKU workflow, którym ten bieg poszedł — `''`, kiedy nic nie biegnie.
   *
   * 2026-08-23 — doszła dla ponownego odpalenia kroku: `workflow` niesie nazwę widoczną dla
   * człowieka, a komenda potrzebuje pliku. Dwa różne fakty, dwa pola — zgadywanie nazwy pliku
   * z tytułu byłoby drugim miejscem z odpowiedzią „gdzie to leży" (T3 §8.3).
   */
  readonly fileName: string;
  /**
   * Folder, w którym pracuje ten bieg — albo `null`, kiedy nic nie biegnie i kiedy człowiek
   * nie wskazał żadnego.
   *
   * 2026-08-18 — PO CO TO POLE POWSTAŁO. Zamknięcie DOWOLNEJ karty wołało `stop_run`, bo
   * domknięcie w `src/sections/run/workspaces-store.ts` ignorowało identyfikator karty
   * (typ go dawał, `stop_run` nie bierze żadnego). Przy drugim otwartym folderze znak `×`
   * stawał się przyciskiem ubijającym CUDZĄ pracę — a to jest błąd finansowy, nie
   * higieniczny (niezmiennik 6). Do rozstrzygnięcia „czy ten bieg należy do tej karty"
   * trzeba jednej rzeczy: gdzie on idzie. Wie to okno, bo samo wysłało ten folder do
   * `run_workflow` (czwarty argument, `AppState::project_for`).
   *
   * Stoi obok `workflow` i `steps`, bo przychodzi tą samą drogą i w tej samej chwili —
   * jednym wywołaniem `nowRunning`, żeby nie było chwili, w której bieg ma już nazwę,
   * a jeszcze nie ma folderu.
   */
  readonly folder: string | null;
  readonly answers: readonly Answer[];

  /**
   * Dokłada paczkę i przycina okno do `LINE_LIMIT`. Oddaje linie, które weszły —
   * dokładnie te obiekty, które przyszły, nigdy ich kopie.
   */
  appendLines: (batch: readonly FeedLine[]) => readonly FeedLine[];

  /**
   * Zapisuje, CO teraz biegnie: nazwę workflow i plan jego kroków.
   *
   * JEDNO wywołanie na oba pola, i to nie jest oszczędność. Nazwa i plan przychodzą tą samą
   * drogą i w tej samej chwili (patrz komentarz przy `workflow`), więc dwa osobne settery
   * dawałyby chwilę, w której pasek loadoutu ma już bloki i jeszcze nie ma podpisu — albo
   * odwrotnie. Ta chwila jest krótka, przez co objawia się jako mignięcie, którego nikt nie
   * umie powtórzyć.
   *
   * `nowRunning('', [])` jest tym samym zdaniem w drugą stronę: bieg zszedł. Wołanie tego
   * z drogi powrotnej Startu jest wymagane przez niezmiennik 16 — Stop, który zostaje na
   * ekranie po biegu, jest kontrolką, która nie ma czego zatrzymać.
   *
   * `folder` jest opcjonalny, bo dwa cudze kryteria wołają tę akcję dwoma argumentami
   * (`stop-becomes-reachable.test.tsx` przez `start()`), a jego brak znaczy dokładnie to,
   * co znaczy `null` po drugiej stronie granicy: „człowiek nie wskazał folderu".
   */
  nowRunning: (
    workflow: string,
    steps: readonly Step[],
    folder?: string | null,
    fileName?: string,
  ) => void;

  /** Zapisuje odpowiedź człowieka. */
  answer: (questionId: number, option: string) => void;
}

export type RunStore = UseBoundStore<StoreApi<RunState>>;

/**
 * Agenci po dołożeniu paczki — TA SAMA tablica, kiedy nikt nowy się w niej nie pojawił.
 *
 * Tożsamość jest tu całą robotą: świeża tablica przy każdej paczce mówi Reactowi, że szyna
 * agentów się zmieniła, dwadzieścia razy na sekundę i przez cały bieg, w którym skład agentów
 * ustala się w pierwszych trzech zdarzeniach.
 */
function withAgents(agents: readonly string[], batch: readonly FeedLine[]): readonly string[] {
  let next: string[] | null = null;
  const seen = new Set(agents);
  for (const line of batch) {
    if (seen.has(line.agent)) continue;
    seen.add(line.agent);
    (next ??= [...agents]).push(line.agent);
  }
  return next ?? agents;
}

/**
 * Siedem stanów kroku jako WARTOŚĆ — typ nie istnieje w czasie wykonania, a wiersz z drutu
 * niesie `state` jako zwykły napis (`src/ipc/types.ts`: `{ kind: 'stepState', …, state: str }`).
 *
 * Zbiór, nie `includes` po tablicy literałów, i nie `state in OBIEKT`: `'constructor' in obiekt`
 * jest prawdą, więc wiersz o stanie `constructor` przestawiłby krok na coś, czego nikt nigdy
 * nie zadeklarował. Ta sama pułapka, dla której `src/ipc/types.ts` trzyma kształty w `Map`.
 */
const STEP_STATES: ReadonlySet<string> = new Set<StepState>([
  'pending',
  'ready',
  'running',
  'succeeded',
  'failed',
  'cancelled',
  'skipped',
]);

/**
 * Kroki po zastosowaniu wierszy `stepState` z paczki — TA SAMA tablica, kiedy nic się nie zmieniło.
 *
 * 2026-08-18 — PO CO TO ISTNIEJE. `nowRunning` miało dwóch wołających, oba w
 * `src/sections/run/io.ts`: plan ze stanami `pending` przy starcie i wyzerowanie na końcu.
 * Żadna inna droga nie przestawiała stanu kroku, więc pasek loadoutu stał na obrysach przez
 * cały bieg, kafelek agenta, który właśnie edytował pliki, mówił „waiting", a SZEŚĆ z siedmiu
 * stanów [ARCHITECTURE §5] było nieosiągalnych. Rodzaj `stepState` na drucie jest tym, co je
 * dowozi; ta funkcja jest jego jedynym konsumentem, żeby „na czym stoi krok" miało jedno
 * miejsce (niezmiennik 13) — pasek i chip na kafelku żyją z tego samego pola.
 *
 * Tożsamość tablicy jest tu robotą, nie mikrooptymalizacją: świeża tablica na każdą paczkę
 * mówi Reactowi, że plan biegu się zmienił, i przelicza pasek loadoutu przy każdej linii
 * (`stripFor` liczy się z `useMemo` po tożsamości `steps`).
 *
 * Wiersz o kroku, którego w planie nie ma, i wiersz o stanie spoza siódemki są PORZUCANE
 * w ciszy, nie rzucane: strona Rusta może wysłać `stepState` kopii kroku albo pod-agenta,
 * a wyjątek tutaj zabrałby cały widok zamiast jednej linii (niezmiennik 5 w duchu).
 */
function withStepStates(steps: readonly Step[], batch: readonly FeedLine[]): readonly Step[] {
  let next: Step[] | null = null;
  for (const line of batch) {
    if (line.kind !== 'stepState') continue;
    if (!STEP_STATES.has(line.state)) continue;
    const state = line.state as StepState;
    const at = (next ?? steps).findIndex((step) => step.id === line.stepId);
    if (at < 0) continue;
    const step = (next ?? steps)[at];
    if (step === undefined || step.state === state) continue;
    (next ??= [...steps])[at] = { ...step, state };
  }
  return next ?? steps;
}

/**
 * Nowy magazyn. Fabryka, nie singleton na poziomie modułu: dwa testy w jednym pliku dzieliłyby
 * stan i drugi z nich czytałby linie pierwszego.
 */
export function createRunStore(): RunStore {
  return create<RunState>()((set) => ({
    lines: [],
    droppedBefore: 0,
    earliestKnownId: null,
    agents: [],
    steps: [],
    workflow: '',
    fileName: '',
    folder: null,
    answers: [],

    appendLines(batch: readonly FeedLine[]): readonly FeedLine[] {
      /* Paczka bez ani jednej linii nie rusza stanu. To nie jest mikrooptymalizacja: kanał
       * woła sink także wtedy, gdy z paczki nie przeżył żaden wiersz (src/ipc/run.ts), a `set`
       * ze świeżą tablicą `lines` jest dla Reacta zmianą i przerysowuje całą historię. */
      if (batch.length === 0) return batch;

      set((state) => {
        const lines = [...state.lines, ...batch];
        /* Obcinamy GŁOWĘ, nigdy ogon. Implementacja obcinająca ogon trzyma dwa tysiące
         * NAJSTARSZYCH linii: długość się zgadza, strumień zamiera w połowie biegu i wygląda
         * to dokładnie jak bieg, który się zatrzymał. */
        const dropped = Math.max(0, lines.length - LINE_LIMIT);
        if (dropped > 0) lines.splice(0, dropped);

        return {
          /* `lines` niesie TE SAME obiekty, które przyszły — `[...]` i `splice` przepisują
           * tablicę, nie wiersze. Kopia wiersza jest poprawna co do wartości i katastrofalna
           * dla Reacta: każdy widoczny wiersz dostaje nową tożsamość na każdą paczkę. */
          lines,
          droppedBefore: state.droppedBefore + dropped,
          /* Dwa pola, dwa różne pytania: ile wypadło (czy „Load earlier" ma po co istnieć)
           * i od czego zacząć prośbę wstecz. */
          earliestKnownId: lines[0]?.id ?? null,
          agents: withAgents(state.agents, batch),
          /* Stan kroku wjeżdża TĄ SAMĄ paczką, co linie, i to jest cała naprawa defektu
           * „sześć z siedmiu stanów nieosiągalnych": jeden konsument, ten sam moment,
           * żadnej drugiej drogi do pola `state` (niezmiennik 13). */
          steps: withStepStates(state.steps, batch),
        };
      });

      return batch;
    },

    nowRunning(
      workflow: string,
      steps: readonly Step[],
      folder: string | null = null,
      fileName = '',
    ): void {
      /* Podstawienie, nie doklejanie: plan biegu przychodzi z grafu w całości i drugi bieg
       * zaczyna się od swojego planu, nie od sumy z poprzednim. `steps` bierzemy dokładnie
       * takie, jakie przyszły — kopia dawałaby paskowi loadoutu nową tożsamość każdego bloku
       * przy każdym wywołaniu, a `stripFor` liczy się z `useMemo` po tej właśnie tożsamości. */
      set({ workflow, steps, folder, fileName });
    },

    answer(questionId: number, option: string): void {
      /* `who: 'you'` — trzy autorytety w całej aplikacji, nie osiem [00-SYNTHESIS §2.2]. */
      set((state) => ({ answers: [...state.answers, { questionId, option, who: 'you' }] }));
    },
  }));
}

/* ─── SESJE PER ZAKRES ───────────────────────────────────────────────────────────────────
 *
 * 2026-08-18 — DECYZJA WŁAŚCICIELA I DEFEKT, KTÓRY ONA OBNAŻA. Zakres pracy („workspace")
 * wybiera się w bocznym menu, a **przełączenie zakresu nie ma prawa zgubić tego, co w innym
 * zakresie idzie**. Do dziś ten plik oddawał JEDEN magazyn na całą aplikację, więc wejście
 * w drugi zakres pokazywałoby okno linii pierwszego: albo pustą historię, albo — co gorsza —
 * `Thinking…` sprzed dwóch minut z cudzego biegu. Wersja z jednym magazynem przechodzi
 * KAŻDY test pisany na jednym zakresie; ta rodzina defektów odsłania się dopiero w chwili,
 * w której człowiek zajrzy gdzie indziej, i wraca do niego z pustą historią.
 *
 * Ten sam akapit stoi w nagłówku `src-tauri/src/workspace.rs` o pompie linii i o karcie:
 * pompa należy do biegu, nie do widoku, a przełączenie jest WYŁĄCZNIE zmianą widoku.
 *
 * KLUCZEM JEST IDENTYFIKATOR ZAKRESU, a nie „numer sesji": kontrakt granicy mówi
 * `id === folder` (`src/state/workspaces-io.ts`), a folder jest tym, co jedzie do
 * `run_workflow`. Dzięki temu ten sam klucz odpowiada na dwa pytania, na które i tak trzeba
 * odpowiedzieć tą samą wartością — „której sesji to linia" i „w którym katalogu pracuje ten
 * bieg" — zamiast dokładać drugie odwzorowanie między nimi (niezmiennik 13).
 */

/**
 * Klucz sesji okna, które nie ma jeszcze ani jednego zakresu.
 *
 * Pusty napis nie zderzy się z żadnym prawdziwym zakresem, bo identyfikatorem zakresu jest
 * kanoniczna ścieżka folderu, a ta nigdy nie jest pusta. Sesja pod tym kluczem jest realna
 * i trwała z premedytacją: bieg nie może w niej wystartować (`launchRun` odmawia bez zakresu),
 * ale trzydzieści testów w tym repo pisze do `useRun` bez ani jednego zakresu — i ma dalej
 * czytać to, co zapisało.
 */
const NO_WORKSPACE = '';

/** Sesje, po jednej na zakres. Powstają na żądanie i ZOSTAJĄ. */
const sessions = new Map<string, RunStore>();

/**
 * Sesja tego zakresu — ta sama przy każdym wywołaniu, przez cały czas życia okna.
 *
 * TU MIESZKA CAŁE „przełączenie nie gubi sesji": magazyn nie powstaje przy renderze ekranu
 * i nie ginie przy jego odmontowaniu, więc bieg zakresu A dopisuje do sesji A także wtedy,
 * gdy człowiek patrzy na zakres B. Wołający, który wie, o którym zakresie mówi — a granica
 * `src/sections/run/io.ts` wie, bo sama wysłała folder do Rusta — pisze TUTAJ, nie przez
 * uchwyt aktywnego zakresu: zapis przez uchwyt trafiałby w zakres, który akurat widać.
 */
export function runFor(workspace: string | null): RunStore {
  const key = workspace ?? NO_WORKSPACE;
  const already = sessions.get(key);
  if (already !== undefined) return already;
  const fresh = createRunStore();
  sessions.set(key, fresh);
  return fresh;
}

/**
 * Zakresy, w których sesja już powstała, w kolejności powstania.
 *
 * Istnieje dla jednego pytania, na które inaczej nie da się odpowiedzieć bez okna: „czy
 * sesja przeżyła przełączenie". Test, który sprawdza to samo, czytając linie po powrocie,
 * nie odróżnia sesji ODTWORZONEJ od sesji ZACHOWANEJ.
 */
export function sessionsAlive(): readonly string[] {
  return [...sessions.keys()];
}

/** Identyfikator zakresu, w którym człowiek właśnie pracuje — albo klucz sesji bez zakresu. */
function activeKey(): string {
  return useWorkspaces.getState().activeId ?? NO_WORKSPACE;
}

/** Sesja zakresu, w którym człowiek właśnie pracuje. */
function activeSession(): RunStore {
  return runFor(activeKey());
}

/** Subskrypcja uchwytu: czyj to nasłuch i jak go odczepić od sesji, do której jest dziś wpięty. */
interface Bond {
  readonly listener: (state: RunState, previous: RunState) => void;
  off: () => void;
}

const bonds: Bond[] = [];

/** Zakres, na którym stoją dzisiejsze wpięcia; `null`, dopóki nikt nie nasłuchuje. */
let bondedTo: string | null = null;

/**
 * Przepina wszystkie nasłuchy na sesję zakresu, który właśnie stał się aktywny, i budzi je.
 *
 * BUDZI, i to nie jest ozdoba: `useSyncExternalStore` czyta migawkę dopiero po powiadomieniu,
 * a przełączenie zakresu zmienia CAŁĄ migawkę bez ruszenia którejkolwiek sesji. Bez tego
 * wywołania ekran zostawałby na linii biegu z poprzedniego zakresu do najbliższej paczki
 * z drutu — czyli w widoku, który myli dwa projekty i sam się nie naprawi, kiedy w nowym
 * zakresie nic nie idzie.
 */
function rebond(): void {
  const key = activeKey();
  if (key === bondedTo) return;
  bondedTo = key;
  const session = activeSession();
  for (const bond of bonds) {
    bond.off();
    bond.off = session.subscribe(bond.listener);
    bond.listener(session.getState(), session.getState());
  }
}

/** Czy pilnujemy już przełączeń zakresu. Jedna subskrypcja na okno, zakładana przy pierwszym nasłuchu. */
let watching = false;

/**
 * Nasłuch na sesji AKTYWNEGO zakresu, przepinany przy każdym przełączeniu.
 *
 * Subskrypcja magazynu zakresów zakłada się dopiero tutaj, a nie przy wczytaniu modułu:
 * ten plik jest importowany przez każdy test, który dotyka biegu, i sięganie po cudzy
 * magazyn w chwili importu wciągałoby granicę Tauri do testów, które o niej nie wiedzą.
 */
function bind(listener: (state: RunState, previous: RunState) => void): () => void {
  if (!watching) {
    watching = true;
    bondedTo = activeKey();
    useWorkspaces.subscribe(rebond);
  }
  const bond: Bond = { listener, off: activeSession().subscribe(listener) };
  bonds.push(bond);
  return () => {
    bond.off();
    const at = bonds.indexOf(bond);
    if (at >= 0) bonds.splice(at, 1);
  };
}

/**
 * UCHWYT DO SESJI AKTYWNEGO ZAKRESU — nie magazyn, tylko okno na ten, który jest teraz.
 *
 * DLACZEGO UCHWYT, A NIE „ten jeden magazyn". Sesji jest tyle, ile zakresów (patrz `runFor`),
 * ale trzydzieści miejsc w tym repo — komponenty i cudze kryteria — pyta o „bieg, który
 * widać", i to pytanie ma dalej jedną odpowiedź. Uchwyt oddaje więc stan, akcje i nasłuch
 * sesji aktywnego zakresu, a przełączenie zakresu przestawia go w całości. Wersja, która
 * KOPIOWAŁABY stan sesji do jednego magazynu, wygląda tak samo i kłamie w chwili, w której
 * dwie sesje żyją naraz: zapis do kopii nie dociera do sesji, a następna paczka z drutu
 * kasuje to, co człowiek właśnie zrobił.
 *
 * Rzutowanie na końcu jest jednym rzutowaniem na cały plik i ma powód: `UseBoundStore` jest
 * typem z DWIEMA sygnaturami wywołania (bez selektora i z selektorem), a jedna implementacja
 * nie jest strukturalnie przypisywalna do przeciążonego typu. Nie ma tu `any` i nie ma
 * tłumienia — jest jedna funkcja, która robi dokładnie to, co robi wiązanie zustanda.
 */
function useRunHandle<U>(selector?: (state: RunState) => U): RunState | U {
  const session = activeSession();
  return selector === undefined ? useStore(session) : useStore(session, selector);
}

export const useRun: RunStore = Object.assign(useRunHandle, {
  getState: (): RunState => activeSession().getState(),
  getInitialState: (): RunState => activeSession().getInitialState(),
  /* Przelotka do `setState` sesji. Podpis `SetStateInternal` też jest przeciążony, więc
   * argumenty jadą dalej nietknięte, a rzutowanie stoi na typie funkcji, nie na wartościach. */
  setState: ((partial: never, replace?: never): void => {
    activeSession().setState(partial, replace);
  }) as StoreApi<RunState>['setState'],
  subscribe: bind,
}) as unknown as RunStore;
