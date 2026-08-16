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
 * # Stan tego pliku: SZKIELET (2026-08-16)
 *
 * Fabryka rzuca. To jest wymagany kształt fazy, w której powstają kryteria: import ma się
 * rozwiązać, a test paść na ZACHOWANIU, nie na wczytywaniu modułu (`AGENTS.md` §2a p. 5).
 */
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

export interface WorkspacesState {
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

export type WorkspacesStore = UseBoundStore<StoreApi<WorkspacesState>>;

/**
 * Nowy magazyn kart nad podanym kanałem anulowania.
 *
 * SZKIELET (2026-08-16): kształt stanu jest już prawdziwy — trzy pola i pięć akcji — więc test
 * się kompiluje i pada na zachowaniu. Brakuje: dokładania kart bez duplikatów, pytania
 * o zamknięcie zależnego od `agents` i jedynej rzeczy, o którą naprawdę chodzi w kryterium 6 —
 * `await cancel(id)` PRZED zdjęciem karty.
 *
 * Podkreślenie w nazwie parametru jest tymczasowe i znika razem z tym ciałem: pod
 * `noUnusedParameters` z `checks/tsconfig.strict.json` argument, którego szkielet jeszcze nie
 * woła, jest błędem typów, a błąd typów w warstwie `before` przewraca bramkę na czymś, co nie
 * jest kryterium.
 */
export function createWorkspacesStore(_cancel: CancelRun): WorkspacesStore {
  throw new Error('the tab store cannot open, switch or close a tab yet');
}
