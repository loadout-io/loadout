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
 * 2026-08-16 — ciała wypełnia T-27. `path` jedzie jako `fileName`, bo to jest SAMA NAZWA pliku,
 * nie ścieżka: katalog rozwiązuje Rust [T3 §8.3]. Front, który dokleiłby katalog sam, byłby
 * drugim miejscem, w którym mieszka odpowiedź na pytanie „gdzie to leży".
 */
import { invoke } from '@tauri-apps/api/core';

import type { Note, WorkflowFile } from '../../state/workflows';
import type { WorkflowEntry } from './list/store';

/** Wszystko, co leży w katalogu workflow, każdy plik ze swoją nazwą. */
export function list(): Promise<WorkflowEntry[]> {
  return invoke<WorkflowEntry[]>('list_workflows');
}

/** uuid v7, wybite po stronie Rusta — ta sama mennica, co w sekcji Agenci. */
export function newId(): Promise<string> {
  return invoke<string>('new_id');
}

/** Wczytuje jeden plik workflow po jego nazwie w katalogu. */
export function load(path: string): Promise<WorkflowFile> {
  return invoke<WorkflowFile>('load_workflow', { fileName: path });
}

/** Zapisuje plik. Odmowa przy problemie żyje po stronie Rusta (`workflow::file::save`). */
export function write(path: string, workflow: WorkflowFile): Promise<void> {
  return invoke<void>('save_workflow', { fileName: path, workflow });
}

/** Usuwa plik workflow z katalogu. */
export function remove(path: string): Promise<void> {
  return invoke<void>('delete_workflow', { fileName: path });
}

/**
 * Uwagi walidatora Rusta (T-12). Frontend ich nie liczy, nie tłumaczy i nie uzupełnia:
 * druga lista uwag byłaby drugim zdaniem o tym samym defekcie, a jedno z dwóch zawsze jest
 * nieaktualne (niezmiennik 13).
 */
export function check(workflow: WorkflowFile): Promise<Note[]> {
  return invoke<Note[]>('check_workflow', { workflow });
}
