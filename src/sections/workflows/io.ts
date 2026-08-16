/* Jedyne miejsce w sekcji Workflow, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, krawędź po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po płótnie i po liście. Oba magazyny tej
 * sekcji — `src/state/workflows.ts` (jeden otwarty dokument) i `src/sections/workflows/list/
 * store.ts` (katalog plików) — biorą swoje `Io` WSTRZYKNIĘTE i nie znają ani jednej nazwy
 * komendy. Autosave, tworzenie i usuwanie mają wtedy jedną krawędź do policzenia; przy dwóch
 * licznik pilnuje jednej, a zapis jedzie drugą i nikt tego nie zauważa.
 *
 * DLACZEGO DWA RÓŻNE TYPY PLIKU W JEDNYM MODULE. `WorkflowEntry` niesie węższe lustro schematu
 * — dokładnie tyle, ile czyta lista — a `WorkflowFile` z `src/state/workflows.ts` jest pełne.
 * Te dwa opisy stoją dziś obok siebie i mają zostać zredukowane do jednego; powód i decyzja są
 * zapisane w nagłówku `list/store.ts` i należą do człowieka, nie do tego pliku.
 *
 * Ciała są jeszcze puste. Szkielet ma się WCZYTAĆ i paść w czasie wykonania — moduł, którego
 * nie ma, daje „Cannot find module", czyli czerwień, której bramka nie liczy (AGENTS.md §2a).
 */
import type { Note, WorkflowFile } from '../../state/workflows';
import type { WorkflowEntry } from './list/store';

/** Wszystko, co leży w katalogu workflow, każdy plik ze swoją nazwą. */
export function list(): Promise<WorkflowEntry[]> {
  throw new Error('not implemented: read the saved workflows');
}

/** uuid v7, wybite po stronie Rusta — ta sama mennica, co w sekcji Agenci. */
export function newId(): Promise<string> {
  throw new Error('not implemented: mint a fresh id');
}

/** Wczytuje jeden plik workflow po jego nazwie w katalogu. */
export function load(path: string): Promise<WorkflowFile> {
  throw new Error('not implemented: read the workflow kept in ' + path);
}

/** Zapisuje plik. Odmowa przy problemie żyje po stronie Rusta (`workflow::file::save`). */
export function write(path: string, workflow: WorkflowFile): Promise<void> {
  throw new Error('not implemented: write ' + workflow.name + ' into ' + path);
}

/** Usuwa plik workflow z katalogu. */
export function remove(path: string): Promise<void> {
  throw new Error('not implemented: drop the file ' + path);
}

/**
 * Uwagi walidatora Rusta (T-12). Frontend ich nie liczy, nie tłumaczy i nie uzupełnia:
 * druga lista uwag byłaby drugim zdaniem o tym samym defekcie, a jedno z dwóch zawsze jest
 * nieaktualne (niezmiennik 13).
 */
export function check(workflow: WorkflowFile): Promise<Note[]> {
  throw new Error('not implemented: ask Rust what is wrong with ' + workflow.name);
}
