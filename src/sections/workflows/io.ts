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

import type { HostMaterial, Note, WorkflowFile } from '../../state/workflows';
import type { Definition } from '../../state/library';
import { definitionsOf, healthyOnly } from '../../state/library';
import type { WorkflowEntry } from './list/store';

/** Wszystko, co leży w katalogu workflow, każdy plik ze swoją nazwą. */
export function listDefinitions(): Promise<Definition<WorkflowEntry>[]> {
  return invoke<Definition<WorkflowEntry>[]>('list_workflows');
}

/** Callery poza ekranem Workflows potrzebują tylko poprawnie wczytanych plików. */
export async function list(): Promise<WorkflowEntry[]> {
  return healthyOnly(definitionsOf(await listDefinitions()));
}

/** uuid v7, wybite po stronie Rusta — ta sama mennica, co w sekcji Agenci. */
export function newId(): Promise<string> {
  return invoke<string>('new_id');
}

/**
 * Otwarty plik razem z rewizją, na której okno go czyta — lustro `commands::workflows`.
 *
 * Para, nie sam plik: zapis, który nie wie, co czytał, nie ma jak odmówić spóźnionemu
 * nadpisaniu, a rewizja dobrana drugim wywołaniem opisywałaby inną chwilę niż to, co człowiek
 * ma przed sobą.
 */
export interface OpenWorkflow {
  workflow: WorkflowFile;
  revision: string;
}

/** Wczytuje jeden plik workflow po jego nazwie w katalogu, razem z jego rewizją. */
export function load(path: string): Promise<OpenWorkflow> {
  return invoke<OpenWorkflow>('load_workflow', { fileName: path });
}

/**
 * Zapisuje plik i oddaje rewizję, którą ma teraz. Odmowa przy problemie żyje po stronie Rusta
 * (`workflow::file::save`) — także ta o spóźnionym zapisie.
 *
 * `expectedRevision` to rewizja, którą okno CZYTAŁO; `null` znaczy „tego pliku ma jeszcze nie
 * być" i tak woła tworzenie oraz duplikowanie. Klucz jedzie zawsze, nawet z `null` w środku:
 * Tauri dopasowuje argumenty po nazwie, więc klucz zdjęty przez `JSON.stringify` byłby
 * wywołaniem ODRZUCONYM, nie mniejszym (`checks/invoke-args.sh`).
 */
export function write(
  path: string,
  workflow: WorkflowFile,
  expectedRevision: string | null,
): Promise<string> {
  return invoke<string>('save_workflow', { fileName: path, workflow, expectedRevision });
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

/**
 * Co folder tego workspace ma do pożyczenia krokom: skille, role z learnings, podagenci.
 *
 * Same nazwy, ani jednego bajtu treści — cudze repozytorium to tekst, którego nikt nie
 * audytował, a wiersz wyboru odpowiada na pytanie „co da się stąd wziąć". Treść czyta dopiero
 * bieg i wyłącznie tę, którą człowiek zaznaczył.
 *
 * `= null`, a nie `folder?: string`, i to nie jest kwestia stylu — powód w całości stoi przy
 * `listSkills` w `src/sections/skills/io.ts`: `JSON.stringify` zdejmuje klucz o wartości
 * `undefined`, a Tauri dopasowuje argumenty PO NAZWIE, więc brakujący klucz jest odrzuconym
 * wywołaniem, nie mniejszym.
 */
export function listHostMaterial(folder: string | null = null): Promise<HostMaterial> {
  return invoke<HostMaterial>('list_host_material', { folder });
}
