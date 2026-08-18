/* Stan kart po stronie UI: co stoi na pasku, co jest na wierzchu i co czeka na potwierdzenie.
 *
 * Karta to folder, w którym pracuje AI (`docs/ARCHITECTURE.md` §6a). Otwarte karty są **stanem
 * UI, nie stanem biegu** (niezmiennik 4): mieszkają w `~/.loadout/ui.json`, a skasowanie tego
 * pliku ma kosztować układ kart i **nic więcej**. Dlatego nie ma tu ani jednego pola, którego
 * nie da się odtworzyć z plików: `id` przychodzi z rejestru po stronie Rusta, `agents` jest
 * bieżącym stanem silnika, a nie zapisem. Identyfikator biegu trzymany TUTAJ jako jedyne
 * miejsce, w którym istnieje, byłby dokładnie tym cichym złamaniem, o które chodzi.
 *
 * CZEGO TU NIE MA I DLACZEGO. Liczby „ile miejsc zajętych" i „ile naraz" nie są stanem kart —
 * są stanem CAŁEJ aplikacji, bo pula jest jedna (niezmiennik 11). Trzymanie ich w magazynie
 * kart robiłoby z niego drugie miejsce, w którym ta sama liczba żyje (niezmiennik 13);
 * pasek dostaje je propsem od tego, kto rozmawia z limiterem.
 *
 * ANULOWANIE WCHODZI WSTRZYKNIĘTE. `cancel` to jedyne wejście tego magazynu do świata poza
 * oknem i dlatego jest argumentem fabryki, a nie importem: kryterium 6 mierzy KOLEJNOŚĆ —
 * karta znika dopiero po tym, jak anulowanie się rozwiązało — a kolejności nie da się zmierzyć
 * na funkcji, której test nie może zatrzymać w połowie.
 *
 * Fabryka i żadnego singletonu obok niej, w odróżnieniu od `src/state/run.ts`: egzemplarz okna
 * potrzebuje prawdziwego kanału anulowania, a ten mieszka w `src/ipc/`, poza blokiem OWNS tego
 * zadania (`AGENTS.md` §7). Singleton zbudowany tutaj z atrapą byłby kontrolką bez handlera
 * przebraną za stan (niezmiennik 16): `×` działałby, a agent by żył.
 *
 * GDZIE TEN PLIK STAŁ DO 2026-08-18 I DLACZEGO SIĘ PRZENIÓSŁ. Cały ten kod mieszkał
 * w `src/state/workspaces.ts` pod nazwą „workspace". Decyzja właściciela z 2026-08-18 rozdzieliła
 * jedno słowo na DWIE rzeczy, których nie wolno zlewać:
 *
 *   - **workspace** — nazwany zakres (nazwa + folder), wybierany w bocznym menu, TRWAŁY na dysku;
 *   - **karta** — bieg WEWNĄTRZ tego zakresu, byt ulotny, żyjący na ekranie Run.
 *
 * Karta jest tym drugim i dlatego stoi tutaj. `src/state/workspaces.ts` trzyma od tego dnia
 * pierwsze znaczenie i re-eksportuje nazwy z tego pliku, żeby ani jeden istniejący import nie
 * padł w trakcie przeprowadzki — ten re-eksport jest SZWEM MIGRACYJNYM, nie drugim źródłem
 * prawdy (niezmiennik 13: definicja jest jedna i jest tutaj), i ma zniknąć w dniu, w którym
 * `src/sections/run/**` zaimportuje `./run-tabs` wprost.
 *
 * Interfejs stanu nazywa się `RunTabsState`, bo nazwa `WorkspacesState` należy teraz do zakresu.
 * Alias `WorkspacesStore` zostaje bez zmiany: to on jest w sygnaturze `createWorkspacesStore`,
 * której nie wolno ruszać, dopóki karty czyta pięć plików sekcji Run.
 */
import { create } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';

/** Jedna karta paska: folder i to, co się w nim właśnie dzieje. */
export interface WorkspaceTab {
  /** Identyfikator z rejestru po stronie Rusta — kanoniczna ścieżka folderu. */
  readonly id: string;
  /** Nazwa folderu, czyli to, co karta mówi o sobie na pasku. */
  readonly name: string;
  /** Pełna ścieżka. Karta ma 34 px, więc pełna ścieżka mieszka w podpowiedzi, nie w napisie. */
  readonly path: string;
  /**
   * Ilu agentów pracuje w tym folderze w tej chwili.
   *
   * Zero znaczy „nic tu nie chodzi" i to jest jedyne pytanie, na które karta w tle odpowiada
   * sama z siebie (§6a reguła 4). Liczba, a nie `boolean`, bo potwierdzenie zamknięcia musi
   * powiedzieć, ilu agentów zatrzymuje: zdanie bez liczby nie jest zdaniem, na podstawie
   * którego da się podjąć decyzję.
   */
  readonly agents: number;
}

/** Karta, o której zamknięcie właśnie zapytaliśmy człowieka. */
export interface PendingClose {
  /** Której karty dotyczy. */
  readonly id: string;
  /** Nazwa folderu — pytanie ma nazywać folder, a nie „bieżącą kartę". */
  readonly name: string;
  /** Ilu agentów pracowało w chwili zadania pytania. */
  readonly agents: number;
}

/**
 * Zatrzymanie biegu w tym folderze.
 *
 * Rozwiązuje się dopiero wtedy, gdy grupa procesów naprawdę nie żyje (niezmiennik 6) —
 * i dlatego zwraca `Promise`, a nie `void`. Wersja synchroniczna nie ma jak powiedzieć
 * „już po wszystkim", więc karta musiałaby zniknąć od razu, zostawiając osieroconego agenta
 * palącego limit u dostawcy.
 */
export type CancelRun = (id: string) => Promise<void>;

export interface RunTabsState {
  /** Karty w kolejności, w jakiej stoją na pasku. */
  readonly tabs: readonly WorkspaceTab[];
  /** Karta na wierzchu; `null`, dopóki żadnej nie ma. */
  readonly activeId: string | null;
  /** Pytanie czekające na odpowiedź; `null`, kiedy o nic nie pytamy. */
  readonly pendingClose: PendingClose | null;

  /** Dokłada kartę i przełącza na nią. Folder, który już ma kartę, jej nie dubluje. */
  open: (tab: WorkspaceTab) => void;

  /**
   * Przełącza kartę. **Wyłącznie zmiana widoku** (§6a reguła 2): nic się nie pauzuje,
   * nie odłącza i nie ginie.
   */
  activate: (id: string) => void;

  /**
   * `×` na karcie. Z pracującymi agentami zadaje pytanie; bez nich zamyka od razu.
   *
   * Pytanie tylko wtedy, kiedy jest o co pytać. Potwierdzenie przy każdym zamknięciu uczy
   * klikać „tak" bez czytania, a wtedy przestaje chronić przed czymkolwiek.
   */
  requestClose: (id: string) => void;

  /** Odpowiedź „nie". Nie zmienia niczego — ani kart, ani tego, co jest na wierzchu. */
  /**
   * Ilu agentow pracuje w tej karcie TERAZ.
   *
   * 2026-08-18 — DLACZEGO TO MUSI ISTNIEC. `agents` bylo pisane WYLACZNIE przy zakladaniu
   * karty, i zawsze zerem. Nikt go nigdy nie podnosil, wiec `requestClose` zawsze wchodzil
   * w galaz „nic nie chodzi": karta z zywym biegiem znikala BEZ pytania i BEZ `cancel(id)`,
   * czyli bez anulowania biegu. Zostawal osierocony agent palacy limit u dostawcy — a to jest
   * blad finansowy, nie higieniczny (niezmiennik 6). Potwierdzenie zamkniecia bylo przy tym
   * kodem nieosiagalnym: zamontowanym, przetestowanym i niedostepnym dla czlowieka.
   *
   * Liczba przychodzi z JEDNEGO zrodla — z listy agentow biegu — i tylko dla karty na wierzchu,
   * bo silnik prowadzi dzis jeden bieg i nie mowi, czyj on jest. Karta, ktora nie jest aktywna,
   * dostaje zero, i to jest prawda o niej, a nie zgadywanie (niezmiennik 17).
   */
  setAgents: (id: string, agents: number) => void;

  dismissClose: () => void;

  /**
   * Odpowiedź „tak": anuluj bieg i **dopiero potem** zdejmij kartę.
   *
   * Kolejność jest całą treścią tej metody. Wersja, która zdejmuje kartę od razu i anuluje
   * w tle, wygląda na ekranie identycznie i zostawia osieroconego agenta — a to jest błąd
   * finansowy, nie higieniczny (niezmiennik 6). Wołane bez czekającego pytania nie robi nic.
   */
  confirmClose: () => Promise<void>;
}

export type WorkspacesStore = UseBoundStore<StoreApi<RunTabsState>>;

/**
 * Karty i to, co jest na wierzchu, po zdjęciu jednej karty.
 *
 * Widok schodzi na sąsiada wyłącznie wtedy, gdy zniknęła karta, na którą człowiek patrzył.
 * Zamknięcie karty w tle nie ma prawa przestawić widoku: to jest ten rodzaj samowoli, po
 * którym `×` klika się z duszą na ramieniu, bo nie wiadomo, gdzie się po nim wyląduje.
 */
function withoutTab(state: RunTabsState, id: string): Pick<RunTabsState, 'tabs' | 'activeId'> {
  const tabs = state.tabs.filter((tab) => tab.id !== id);
  if (state.activeId !== id) return { tabs, activeId: state.activeId };

  /* Sąsiad z prawej, a po ostatniej karcie ten z lewej — czyli miejsce, w którym stała
   * zamknięta karta. Skok na początek paska gubi kontekst tym mocniej, im więcej kart jest
   * otwartych, a otwiera się je właśnie po to, żeby ich było kilka. */
  const closed = state.tabs.findIndex((tab) => tab.id === id);
  const next = tabs[Math.min(closed, tabs.length - 1)];
  return { tabs, activeId: next === undefined ? null : next.id };
}

/**
 * Nowy magazyn kart nad podanym kanałem anulowania.
 *
 * `cancel` wchodzi argumentem, a nie importem, i to jest cała konstrukcja kryterium 6: mierzy
 * ono KOLEJNOŚĆ — karta znika dopiero po tym, jak anulowanie się rozwiązało — a kolejności nie
 * da się zmierzyć na funkcji, której test nie może zatrzymać w połowie.
 */
export function createWorkspacesStore(cancel: CancelRun): WorkspacesStore {
  /* Które zamknięcie jest w toku. Poza stanem widoku, bo widok nie ma o czym z tym rozmawiać:
   * to jest wyłącznie zapadka przed drugim sygnałem stopu dla tego samego biegu. Dwa sygnały
   * to dwie eskalacje ścigające się o tego samego agenta (niezmiennik 6), a klawisz Enter
   * przytrzymany na potwierdzeniu wysyła je tyle razy, ile zdąży. */
  let closing: string | null = null;

  return create<RunTabsState>()((set, get) => ({
    tabs: [],
    activeId: null,
    pendingClose: null,

    open: (tab) => {
      const { tabs } = get();
      /* Jeden folder = jedna karta (§6a reguła 1). Folder, który już ma kartę, przełącza na
       * nią — dwa biegi w jednym katalogu kolidowałyby na plikach, a kopia per krok chroni
       * kroki między sobą, nigdy biegi między sobą. */
      const already = tabs.some((open) => open.id === tab.id);
      set({ tabs: already ? tabs : [...tabs, tab], activeId: tab.id });
    },

    activate: (id) => {
      /* Wyłącznie zmiana widoku (§6a reguła 2). Nic się tu nie pauzuje, nie odłącza i nie
       * ginie: pompa linii wisi na karcie po stronie Rusta, nie na tym polu. */
      if (!get().tabs.some((tab) => tab.id === id)) return;
      set({ activeId: id });
    },

    requestClose: (id) => {
      const tab = get().tabs.find((open) => open.id === id);
      if (tab === undefined) return;

      /* Nie ma o co pytać, kiedy nic nie chodzi. Potwierdzenie przy KAŻDYM zamknięciu uczy
       * klikać „tak" bez czytania, a wtedy nie chroni już przed niczym — także przed tym
       * jednym zamknięciem, przy którym pracowało trzech agentów. */
      if (tab.agents === 0) {
        set(withoutTab(get(), id));
        return;
      }
      set({ pendingClose: { id: tab.id, name: tab.name, agents: tab.agents } });
    },

    setAgents: (id, agents) => {
      const tabs = get().tabs;
      const at = tabs.findIndex((tab) => tab.id === id);
      /* Bez zmiany nie ruszamy stanu: `set` ze swieza tablica jest dla Reacta zmiana i
       * przerysowuje caly pasek kart przy kazdej linii biegu. */
      if (at < 0 || tabs[at]?.agents === agents) return;
      const next = [...tabs];
      const tab = tabs[at];
      if (tab === undefined) return;
      next[at] = { ...tab, agents };
      set({ tabs: next });
    },

    dismissClose: () => {
      set({ pendingClose: null });
    },

    confirmClose: async () => {
      const { pendingClose } = get();
      if (pendingClose === null || closing !== null) return;

      closing = pendingClose.id;
      try {
        /* NAJPIERW anulowanie, do końca. Wersja, która zdejmuje kartę od razu i anuluje w tle,
         * wygląda na ekranie identycznie i zostawia osieroconego agenta palącego limit
         * u dostawcy — to jest błąd finansowy, nie higieniczny (niezmiennik 6). Pytanie zostaje
         * na ekranie przez cały ten czas, bo bieg wciąż się zwija i nie ma o czym milczeć. */
        await cancel(pendingClose.id);
      } finally {
        closing = null;
      }

      /* Stan czytany PONOWNIE, już po anulowaniu: minęła cała runda zwijania biegu i karty
       * mogły się w tym czasie zmienić. Zapisanie tu migawki sprzed `await` cofałoby wszystko,
       * co zdarzyło się w międzyczasie. */
      set({ ...withoutTab(get(), pendingClose.id), pendingClose: null });
    },
  }));
}
