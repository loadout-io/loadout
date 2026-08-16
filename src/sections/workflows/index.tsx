/* Ekran sekcji Workflows — SZKIELET FAZY KONTRAKTU, jeszcze bez ciała.
 *
 * Puste ciało jest tu wymagane, nie przeoczone. Szkielet ma się WCZYTAĆ — żeby odkrywanie
 * z `src/ui/screens.ts` w ogóle go znalazło i żeby kryterium się skompilowało — i paść
 * w czasie WYKONANIA. Moduł, którego nie ma, daje „Cannot find module", czyli czerwień,
 * której bramka nie liczy (AGENTS.md §2a). `throw` jest dokładnym odpowiednikiem `todo!()`
 * z Rusta; ta sama konwencja stoi już w `src/sections/skills/io.ts` i `sections/memory/io.ts`.
 *
 * DLACZEGO NIE PUSTY `<div/>`. `tasks/T-26.md` przy pierwszym kryterium wypisuje go z nazwy
 * jako słabą asercję, którą to kryterium ma złapać: na pustym `<div/>` przechodzi
 * `not.toContain(sectionEntry('workflows').empty)`. Szkielet, na którym słaba asercja jest
 * zielona, uczy fazę wykonawczą, że kryterium jest już spełnione.
 *
 * CO SKŁADA FAZA WYKONAWCZA. Ekran jest CIENKI: bierze `createWorkflowListStore` (T-14) i
 * renderuje `WorkflowList` (T-14). Druga lista, drugi kafelek albo druga ścieżka tworzenia
 * to drugie miejsce prawdy (niezmiennik 23) — te komponenty są wylądowane i mają własne
 * kryteria. Brakuje wyłącznie tego, co leży MIĘDZY komponentem a sekcją.
 *
 * ZMIERZONE 2026-08-16, PRZECZYTAJ ZANIM NAPISZESZ CIAŁO. `renderToStaticMarkup` jest
 * rendererem serwerowym, a zustand 5 podaje mu `getInitialState` jako migawkę serwerową
 * (`node_modules/zustand/esm/react.mjs`). Ekran czytający magazyn hakiem zustanda pokazuje
 * więc stan Z CHWILI UTWORZENIA magazynu i NIGDY tego, co test zasiał przez `load()` —
 * sonda: `getState()` mówiło dwie pozycje, markup mówił zero. W repo nie ma `jsdom` ani
 * `@testing-library/react`, więc innego renderera nie będzie. Magazyn czyta się tu przez
 * `useSyncExternalStore(store.subscribe, store.getState, store.getState)`: trzeci argument
 * jest STANEM BIEŻĄCYM, bo ta aplikacja nigdy nie hydratuje serwerowego HTML-a.
 */
import type { ReactElement } from 'react';
import { createWorkflowListStore } from './list/store';

/** Magazyn listy workflow — dokładnie ten, który oddaje `createWorkflowListStore`. */
export type WorkflowListStore = ReturnType<typeof createWorkflowListStore>;

export interface WorkflowsScreenProps {
  /**
   * Magazyn ekranu. Bez propsu ekran bierze swój prawdziwy, z propsem ten z testu —
   * dokładnie tak, jak powłoka przyjmuje opcjonalne `screens` (`src/App.tsx`).
   */
  store?: WorkflowListStore;
}

export default function WorkflowsScreen(_props: WorkflowsScreenProps): ReactElement {
  throw new Error('not implemented: show the workflows that lie on disk');
}
