/* Ekran sekcji Workflows: lista tego, co leży w katalogu workflow.
 *
 * CIENKI Z ZAŁOŻENIA I TO JEST CAŁA TREŚĆ TEGO PLIKU. Nagłówek z licznikiem, przycisk
 * tworzenia, zaproszenie przy zerze i pytanie przed usunięciem stoją już w `WorkflowList`
 * (T-14) i mają tam własne kryteria. Drugi nagłówek albo drugie zaproszenie tutaj byłoby
 * drugim miejscem prawdy (niezmiennik 23), a w markupie DRUGIM przyciskiem tworzenia, czyli
 * drugą ścieżką, którą powstaje plik (niezmiennik 16). Między komponentem a sekcją brakowało
 * dokładnie dwóch rzeczy: magazynu i tego pliku.
 *
 * Płótno jest świadomie poza zakresem: przejście z listy na płótno domyka T-15, a kafelek,
 * który nigdzie nie prowadzi, nie jest tu przyciskiem właśnie dlatego (`list/tile.tsx`).
 *
 * DLACZEGO MAGAZYN NIE JEST CZYTANY HAKIEM ZUSTANDA — zmierzone 2026-08-16.
 * `renderToStaticMarkup` jest rendererem serwerowym, a zustand 5 podaje mu `getInitialState`
 * jako migawkę serwerową (`node_modules/zustand/esm/react.mjs`). Ekran czytający magazyn
 * hakiem `useStore` pokazywałby więc stan Z CHWILI UTWORZENIA magazynu i nigdy tego, co
 * wczytał `load()`: sonda mówiła dwie pozycje z `getState()` i zero w markupie. Trzecim
 * argumentem `useSyncExternalStore` jest tu dlatego STAN BIEŻĄCY — ta aplikacja nigdy nie
 * hydratuje serwerowego HTML-a, więc powód, dla którego React chce tam stanu początkowego
 * (zgodność hydratacji), tutaj nie istnieje.
 *
 * DLACZEGO ADAPTER DYSKU STOI W TYM PLIKU. Sekcja powinna mieć swój `io.ts` — tak jak mają
 * `sections/skills/` i `sections/memory/` — ale `src/sections/agents/io.ts` i jego odpowiednik
 * dla workflow nie istnieją, a warstwę IPC dowozi dopiero T-07 (zmierzone 2026-08-16: zero
 * `#[tauri::command]` w całym drzewie Rusta, brak `src/ipc.ts`). Adapter jest więc TUTAJ,
 * w jednym miejscu, i jest jedyną rzeczą w tej sekcji, która wie, że dysku jeszcze nie ma.
 * Przeniesienie go do `list/io.ts` jest zapisem poza blokiem OWNS tego zadania (AGENTS.md §7)
 * — zgłoszone człowiekowi razem z tym plikiem.
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import type { WorkflowListIo } from './list/store';
import { createWorkflowListStore } from './list/store';
import { WorkflowList } from './list/workflow-list';

/** Magazyn listy workflow — dokładnie ten, który oddaje `createWorkflowListStore`. */
export type WorkflowListStore = ReturnType<typeof createWorkflowListStore>;

export interface WorkflowsScreenProps {
  /**
   * Magazyn ekranu. Bez propsu ekran bierze swój prawdziwy, z propsem ten z testu —
   * dokładnie tak, jak powłoka przyjmuje opcjonalne `screens` (`src/App.tsx`).
   */
  store?: WorkflowListStore;
}

/* Zdanie odmowy jedzie do tego, kto wołał, jako `Error`. Sekcja nie ma go dziś gdzie pokazać
 * — obsługa błędów plików należy do T-12 — ale zapis, który po cichu KOŃCZY SIĘ SUKCESEM,
 * byłby kłamstwem o tym, co leży na dysku (niezmiennik 4), a to jest gorsze niż cisza. */
const NO_DISK = 'Loadout cannot reach the folder that holds workflows yet.';

/* Odczyt katalogu odpowiada „nic tam nie leży" i to jest dziś prawda, nie zaślepka: katalog
 * workflow zakłada strona Rusta, której jeszcze nie ma, więc nie ma tam ani jednego pliku.
 * Odmowa zamiast pustej listy dawałaby za to odrzuconą obietnicę bez ani jednego miejsca,
 * które by ją przechwyciło — czyli ostrzeżenie w konsoli zamiast pustej listy na ekranie. */
const DISK: WorkflowListIo = {
  list: () => Promise.resolve([]),
  newId: () => Promise.reject(new Error(NO_DISK)),
  write: () => Promise.reject(new Error(NO_DISK)),
  remove: () => Promise.reject(new Error(NO_DISK)),
};

/* Prawdziwy magazyn sekcji powstaje RAZ, przy wczytaniu modułu, a nie przy renderze: magazyn
 * budowany w ciele komponentu gubiłby całą zawartość ekranu przy każdym przemontowaniu. */
const OWN_STORE = createWorkflowListStore(DISK);

export default function WorkflowsScreen({ store = OWN_STORE }: WorkflowsScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  /* Katalog czytamy przy wejściu na sekcję. Pliki są prawdą, a ekran jest ich widokiem
   * (niezmiennik 4) — lista, która nigdy nie pyta dysku, pokazuje to, co pamięta z ostatniego
   * zapisu tego okna. `store` w zależnościach, bo z propsem może przyjechać inny magazyn. */
  useEffect(() => {
    void store.getState().load();
  }, [store]);

  /* Cały stan idzie jako `actions`: `WorkflowListState` rozszerza `WorkflowListActions`, więc
   * to jest TEN SAM obiekt, który magazyn wystawia — a nie jego przepisana kopia. */
  return (
    <WorkflowList
      workflows={state.workflows}
      pendingDeleteId={state.pendingDeleteId}
      actions={state}
    />
  );
}
