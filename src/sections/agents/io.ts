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

/** Zapisuje definicję agenta. Duplikat to nowy PLIK, nie wiersz żyjący na ekranie. */
export function save(agent: Agent): Promise<void> {
  return invoke<void>('save_agent', { agent });
}

/** Usuwa agenta po identyfikatorze — stabilnym przez zmianę nazwy, w odróżnieniu od pliku. */
export function remove(id: string): Promise<void> {
  return invoke<void>('delete_agent', { id });
}
