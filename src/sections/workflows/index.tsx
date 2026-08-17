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
 * SKĄD BIERZE SIĘ ADAPTER DYSKU — poprawione 2026-08-17 (T-29, runda naprawcza).
 * Stała tu wcześniej zaślepka `DISK`, której `newId` i `write` ODMAWIAŁY zawsze: powstała
 * 2026-08-16, kiedy w drzewie Rusta nie było ani jednego `#[tauri::command]`, a `io.ts` tej
 * sekcji jeszcze nie istniał. Dziś istnieje i wywołuje prawdziwe komendy (`list_workflows`,
 * `new_id`, `save_workflow`, `delete_workflow`), więc zaślepka przestała opisywać świat
 * i zaczęła go ZASŁANIAĆ: `＋ Create` w oknie dochodził do odrzuconej obietnicy i nie
 * zostawiał ani pliku, ani kafelka — czyli był tym martwym przyciskiem, o którym mówi
 * niezmiennik 16, tylko z handlerem. Sekcja bierze więc `io.ts` i nie zna ani jednej nazwy
 * komendy sama (niezmiennik 23).
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import * as io from './io';
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

/* Prawdziwy magazyn sekcji powstaje RAZ, przy wczytaniu modułu, a nie przy renderze: magazyn
 * budowany w ciele komponentu gubiłby całą zawartość ekranu przy każdym przemontowaniu.
 *
 * Cztery funkcje wymienione po nazwie, a nie `io` w całości: `WorkflowListIo` jest kontraktem
 * tego magazynu, a moduł `io.ts` niesie też `load` i `check`, których lista nie używa. Podanie
 * całego modułu przeszłoby kompilację i zamieniłoby ten wiersz w miejsce, w którym nie widać,
 * ILE granicy naprawdę dotyka ten ekran. */
const OWN_STORE = createWorkflowListStore({
  list: io.list,
  newId: io.newId,
  write: io.write,
  remove: io.remove,
});

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
