/* Jedyne miejsce w sekcji Agenci, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, krawędź po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po magazynie i po formularzu. Magazyn
 * (`src/state/agents.ts`) dostaje ten moduł WSTRZYKNIĘTY jako `AgentsIo` i dzięki temu jego
 * testy podstawiają atrapę bez dotykania transportu. Zdanie „edycja kroku nie zapisuje agenta"
 * ma sens tylko wtedy, kiedy jest jedna krawędź, przez którą da się zapisać cokolwiek — dwie
 * drogi do Rusta znaczą, że asercja pilnuje jednej z nich, a zapis jedzie drugą.
 *
 * Kształt jest lustrem `AgentsIo` z `src/state/agents.ts` i tak ma zostać: funkcja dopisana
 * tutaj bez pozycji tam jest funkcją, której magazyn nie umie zawołać.
 *
 * 2026-08-16 — ciała wypełnia T-27. Nazwy komend są dosłownie te z `src-tauri/commands.golden.txt`
 * i muszą takie zostać: ten sam plik czyta po drugiej stronie granicy `ipc_commands_registered.rs`,
 * więc nazwa sklejona tutaj ze zmiennej albo przepisana z pamięci rozjeżdża się w ciszy —
 * `invoke` na nieistniejącą komendę odmawia dopiero pod palcem użytkownika.
 */
import { invoke } from '@tauri-apps/api/core';

import type { Agent } from '../../state/agents';
import type { Definition } from '../../state/library';
import { definitionsOf, healthyOnly } from '../../state/library';

/** Wszyscy zapisani agenci, po jednym na plik w bibliotece. */
export function listDefinitions(): Promise<Definition<Agent>[]> {
  return invoke<Definition<Agent>[]>('list_agents');
}

/** Callery poza ekranem Agents potrzebują tylko zdrowych zapisanych agentów. */
export async function list(): Promise<Agent[]> {
  return healthyOnly(definitionsOf(await listDefinitions()));
}

/**
 * Świeży identyfikator, wybity po stronie Rusta.
 *
 * Nie `crypto.randomUUID()`: tamto daje v4, czyli liczbę losową, a tutaj musi być v7 —
 * sortowalne po czasie [T4 §5.1]. Mennica stoi tam, gdzie v7 już jest.
 */
export function newId(): Promise<string> {
  return invoke<string>('new_id');
}

/**
 * Zapisuje definicję agenta i oddaje rewizję, którą ma teraz jego plik. Duplikat to nowy PLIK,
 * nie wiersz żyjący na ekranie.
 *
 * `expectedRevision` to rewizja, którą okno CZYTAŁO dla tego agenta; `null` znaczy „tego pliku
 * ma jeszcze nie być". Klucz jedzie zawsze, nawet z `null` w środku — Tauri dopasowuje
 * argumenty po nazwie, więc klucz zdjęty przez `JSON.stringify` byłby wywołaniem ODRZUCONYM.
 */
export function save(agent: Agent, expectedRevision: string | null): Promise<string> {
  return invoke<string>('save_agent', { agent, expectedRevision });
}

/**
 * Rewizja pliku tego agenta, prosto z biblioteki — albo `null`, kiedy jej tam nie ma.
 *
 * PO CO TO ISTNIEJE. Edytor workflow też zapisuje agenta (naprawa „Save to the agent"
 * w `state/workflows.ts`), a rewizji pliku nie trzyma i trzymać nie powinien: to nie jest fakt
 * o otwartym workflow. Zamiast przewlekać ją przez płótno i trzy panele, czytamy bibliotekę
 * tuż przed zapisem — dokładnie tak, jak magazyn listy czyta katalog tuż przed wyborem wolnej
 * nazwy pliku. Okno między odczytem a zapisem zamyka i tak Rust: to on porównuje bajty.
 */
export async function revisionOf(id: string): Promise<string | null> {
  const found = definitionsOf(await listDefinitions()).find(
    (definition) => definition.kind === 'healthy' && definition.value.id === id,
  );
  return found?.kind === 'healthy' ? (found.revision ?? null) : null;
}

/** Usuwa agenta po identyfikatorze — stabilnym przez zmianę nazwy, w odróżnieniu od pliku. */
export function remove(id: string): Promise<void> {
  return invoke<void>('delete_agent', { id });
}
