/* Workspace: nazwany zakres, w którym pracują agenci. JEDNA odpowiedź na pytanie „gdzie pracujemy".
 *
 * DECYZJA WŁAŚCICIELA, 2026-08-18. Do tego dnia folder pracy wybierało się **systemowym oknem,
 * przy każdym uruchomieniu biegu**: `launchRun` pytał o katalog, kiedy żadna karta nie była
 * otwarta. Zdanie właściciela po zobaczeniu tego okna brzmiało „mega chujnia" i było trafne —
 * wybór folderu jest decyzją o PROJEKCIE, podejmowaną raz, a nie czynnością powtarzaną przed
 * każdą pracą. Workspace jest więc nazwanym zakresem, wybieranym w bocznym menu, i to on mówi,
 * gdzie pracują agenci.
 *
 * CO WORKSPACE ZAKRESUJE, A CZEGO NIE. Wyłącznie **folder pracy i żywą sesję** (strumień, karty
 * biegów, stan biegu). Workflow, agenci, umiejętności i pamięć zostają GLOBALNE w `~/.loadout` —
 * powód stoi przy `src-tauri/src/commands/workspaces.rs` i jest nazwany: umiejętności piszą do
 * `~/.claude/skills` i `~/.agents/skills`, czyli do konfiguracji NARZĘDZI człowieka, a nie do
 * jego projektu. Zero migracji, zero drugiego katalogu.
 *
 * WORKSPACE ≠ KARTA, i to jest jedyna rzecz, którą trzeba tu czytać uważnie. Do 2026-08-18 ten
 * plik modelował KARTY FOLDERÓW (`WorkspaceTab`, `open`, `requestClose`) i słowo „workspace"
 * znaczyło w nim coś innego niż dziś. Ten kod nie zniknął — przeniósł się do
 * `src/state/run-tabs.ts` i jest re-eksportowany niżej, żeby przeprowadzka nie wywróciła pięciu
 * plików sekcji Run w trakcie. Rozdział znaczeń:
 *
 *   - **workspace** (ten plik) — nazwa + folder, wybierany w bocznym menu, TRWAŁY na dysku,
 *     jeden na całą aplikację jako „aktywny";
 *   - **karta** (`run-tabs.ts`) — bieg WEWNĄTRZ workspace, byt ulotny, żyje na ekranie Run.
 *
 * DYSK-PIERWSZY, WSZĘDZIE. `add`, `rename` i `remove` zmieniają stan DOPIERO po potwierdzeniu
 * z dysku i oddają `boolean`, żeby wołający zamknął formularz tylko wtedy, kiedy plik naprawdę
 * się zapisał. Odwrotna kolejność to defekt, który już raz w tym repo wystąpił: agent zniknięty
 * z listy przy NIEUDANYM usunięciu wracał po restarcie, bo okno uwierzyło sobie, a nie plikowi.
 *
 * ODMOWY IDĄ PRZEZ `why()`, NIGDY PRZEZ `instanceof Error`. Tauri odrzuca NAPISEM
 * (`src-tauri/src/ipc.rs` robi `.map_err(|e| e.to_string())`), więc warunek `error instanceof
 * Error` jest tu ZAWSZE fałszywy — stał w siedmiu miejscach i kasował każdą precyzyjną odmowę,
 * jaką Rust naprawdę pisze („The folder … is not there, so Loadout did not add it").
 */
import { create } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';

import { why } from '../ipc/why';
import { deleteWorkspace, listWorkspaces, saveWorkspace } from './workspaces-io';

/* SZEW MIGRACYJNY, 2026-08-18. Karty przeniosły się do `./run-tabs`, a te nazwy czyta dziś pięć
 * plików sekcji Run i trzy testy. Re-eksport nie jest drugą definicją (niezmiennik 13: ciało
 * stoi w jednym pliku) — jest przekierowaniem, które ma zniknąć w dniu, w którym te importy
 * wskażą `./run-tabs` wprost. Zapisane jako dług, nie jako rozwiązanie. */
export type {
  CancelRun,
  PendingClose,
  RunTabsState,
  WorkspaceTab,
  WorkspacesStore,
} from './run-tabs';
export { createWorkspacesStore } from './run-tabs';

/**
 * Jeden nazwany zakres pracy.
 *
 * `id` JEST ścieżką folderu (`WorkspaceWire` po stronie Rusta), i to nie jest oszczędność: jeden
 * folder = jeden workspace, więc ścieżka jest naturalnym kluczem i nie da się zapisać dwóch
 * wpisów o tym samym folderze przez pomyłkę. `folder` stoi osobno, bo okno nie ma prawa zakładać,
 * że klucz jest ścieżką — dzień, w którym klucz przestanie nią być, ma zmienić jeden plik.
 */
export interface Workspace {
  /** Klucz wpisu. Dziś równy `folder`. */
  readonly id: string;
  /** Nazwa nadana przez człowieka. To ona stoi w przełączniku bocznego menu. */
  readonly name: string;
  /** Folder pracy — jedyna rzecz, którą ten wpis naprawdę niesie. */
  readonly folder: string;
}

export interface WorkspacesState {
  /** Wszystkie zakresy, w kolejności zapisu na dysku. Pusta lista jest poprawnym stanem. */
  readonly all: readonly Workspace[];
  /** Zakres, w którym pracujemy; `null`, dopóki żadnego nie ma. */
  readonly activeId: string | null;
  /** Zdanie, którym dysk odmówił — dla człowieka, słowo w słowo od Rusta. `null`, kiedy nie odmówił. */
  readonly said: string | null;

  /** Czyta listę z dysku. Wołane raz, przy starcie okna. */
  load: () => Promise<void>;

  /** Dokłada zakres albo zmienia nazwę istniejącego (klucz to folder). `true` = dysk potwierdził. */
  add: (name: string, folder: string) => Promise<boolean>;

  /** Zmienia nazwę zakresu, który już jest na liście. `true` = dysk potwierdził. */
  rename: (id: string, name: string) => Promise<boolean>;

  /** Zdejmuje zakres z listy. Folderu nie dotyka. `true` = dysk potwierdził. */
  remove: (id: string) => Promise<boolean>;

  /**
   * Przełącza zakres. **Wyłącznie zmiana widoku** — dysku nie dotyka ani razu.
   *
   * Przełączenie nie ma prawa zgubić sesji i to jest wymóg twardy właściciela z 2026-08-18.
   * Ten magazyn go spełnia przez to, czego NIE robi: nie zatrzymuje biegu, nie odłącza pompy
   * linii i nie kasuje kart. Pompa należy do karty po stronie Rusta
   * (`src-tauri/src/workspace.rs`), nie do tego pola.
   */
  activate: (id: string) => void;

  /** Człowiek przeczytał odmowę. Nic poza zdaniem nie znika. */
  dismiss: () => void;
}

/**
 * Który zakres ma być aktywny, kiedy lista właśnie się zmieniła.
 *
 * JEDEN NIEZMIENNIK NA CAŁY MAGAZYN: `activeId` wskazuje na wpis, który ISTNIEJE, albo jest
 * `null`, kiedy nie ma żadnego. Bez tego zdania każda operacja musiałaby pamiętać własny
 * przypadek brzegowy, a `activeWorkspace()` oddawałoby `null` przy niepustej liście — czyli
 * ekran Run bez folderu przy trzech workspace'ach w menu.
 *
 * Pierwszy z listy, a nie `null`, kiedy życzenie przepadło: to samo zdanie obsługuje start okna
 * (po restarcie `activeId` nie istnieje, bo NIE JEST zapisywany na dysku — wybór widoku nie jest
 * stanem trwałym) i usunięcie aktywnego zakresu.
 */
function pick(all: readonly Workspace[], wanted: string | null): string | null {
  if (wanted !== null && all.some((one) => one.id === wanted)) return wanted;
  return all[0]?.id ?? null;
}

export const useWorkspaces: UseBoundStore<StoreApi<WorkspacesState>> = create<WorkspacesState>()(
  (set, get) => ({
    all: [],
    activeId: null,
    said: null,

    load: async () => {
      try {
        const all = await listWorkspaces();
        /* Pusta lista NIE jest błędem: na świeżej maszynie pliku nie ma i Rust oddaje `[]`.
         * Przełącznik pokaże wtedy zaproszenie do dodania pierwszego zakresu (DESIGN §6),
         * a nie zdanie o awarii. */
        set({ all, activeId: pick(all, get().activeId), said: null });
      } catch (error) {
        set({ said: why(error, 'Loadout could not read your list of workspaces.') });
      }
    },

    add: async (name, folder) => {
      try {
        /* DYSK PIERWSZY. Stan bierzemy z odpowiedzi, nie z argumentów: Rust przycina nazwę,
         * odmawia folderowi, którego nie ma, i przy DRUGIM zapisie tego samego folderu zmienia
         * nazwę zamiast dokładać wiersz. Lista złożona tutaj z `[...all, { name, folder }]`
         * pokazywałaby duplikat, którego w pliku nie ma. */
        const all = await saveWorkspace({ name, folder });
        /* Nowy zakres staje się aktywny, bo dodanie go JEST zdaniem „chcę tu pracować".
         * Wersja, która tylko dokłada wiersz, zostawia człowieka z listą i bez skutku. */
        set({ all, activeId: pick(all, folder), said: null });
        return true;
      } catch (error) {
        set({ said: why(error, 'Loadout could not add that workspace.') });
        return false;
      }
    },

    rename: async (id, name) => {
      const had = get().all.find((one) => one.id === id);
      /* Zmiana nazwy jedzie tą samą komendą co dodanie (klucz to folder), więc bez folderu nie
       * ma czego zapisać. Nieznany identyfikator to nie awaria dysku i nie wolno o nim mówić
       * zdaniem o dysku: lista zmieniła się pod ręką człowieka i to jest cała treść odmowy. */
      if (had === undefined) {
        set({ said: 'That workspace is no longer on the list, so Loadout did not rename it.' });
        return false;
      }
      try {
        const all = await saveWorkspace({ name, folder: had.folder });
        set({ all, activeId: pick(all, get().activeId), said: null });
        return true;
      } catch (error) {
        set({ said: why(error, 'Loadout could not rename that workspace.') });
        return false;
      }
    },

    remove: async (id) => {
      try {
        const all = await deleteWorkspace({ id });
        /* Zdjęcie AKTYWNEGO zakresu zwalnia życzenie — wtedy `pick` bierze pierwszy z listy albo
         * `null`. Zdjęcie zakresu w tle nie ma prawa przestawić widoku: to jest ta samowola, po
         * której „Remove" klika się z duszą na ramieniu, bo nie wiadomo, gdzie się wyląduje. */
        const wanted = get().activeId === id ? null : get().activeId;
        set({ all, activeId: pick(all, wanted), said: null });
        return true;
      } catch (error) {
        set({ said: why(error, 'Loadout could not remove that workspace.') });
        return false;
      }
    },

    activate: (id) => {
      /* Zakres, którego nie ma na liście, nie staje się aktywny — inaczej `activeWorkspace()`
       * oddawałoby wpis, którego nikt nie zapisał. Dysku ta metoda nie dotyka ani razu. */
      if (!get().all.some((one) => one.id === id)) return;
      set({ activeId: id });
    },

    dismiss: () => {
      set({ said: null });
    },
  }),
);

/**
 * Aktywny workspace albo `null`. JEDNA definicja „gdzie pracujemy" na całe repo.
 *
 * FUNKCJA MODUŁOWA, NIE HAK, i to jest cała jej konstrukcja: czytają ją rzeczy spoza drzewa
 * Reacta — kontrolka startu przed wysłaniem folderu do Rusta, kanał zdarzeń, kod uruchamiający
 * bieg. Hak dałby tę odpowiedź wyłącznie w czasie renderu, więc obok musiałaby powstać druga
 * droga do tej samej prawdy (niezmiennik 13).
 *
 * `activeWorkspace()?.folder` zastępuje `activeFolder()` z `src/sections/run/workspaces-store.ts`.
 * `null` znaczy „człowiek nie wskazał jeszcze zakresu", a odpowiedź na to pytanie należy do
 * Rusta (`AppState::project_for` bierze wtedy katalog, pod którym wstała aplikacja) — front,
 * który podstawiłby tu własną domyślną ścieżkę, byłby drugim miejscem, w którym mieszka
 * ta decyzja.
 */
export function activeWorkspace(): Workspace | null {
  const { all, activeId } = useWorkspaces.getState();
  if (activeId === null) return null;
  return all.find((one) => one.id === activeId) ?? null;
}
