import { activeWorkspace } from '../../state/workspaces';
import { invoke } from '@tauri-apps/api/core';
import { filledAgents } from '../../ipc/filled-agents';
import type {
  ApplyRequest,
  CompareRequest,
  Comparison,
  ImportPreview,
  ImportReceipt,
} from './setup';

/** Skan cudzego projektu. `folder` to projekt, do którego biblioteki ten import ZAPISZE —
 *  bez niego skan nie ma jak powiedzieć, czego ta biblioteka już ma (`scan_setup` w `ipc.rs`).
 *
 *  Klucze wypisane wprost, tym samym powodem, co przy `compareCopies` niżej. */
export function scanSetup(
  workspace: string,
  folder: string | null = activeWorkspace()?.folder ?? null,
): Promise<ImportPreview> {
  return invoke<ImportPreview>('scan_setup', { folder, workspace });
}

/** Zapis planu. Odpowiedź przechodzi przez bramkę kształtu, bo `invoke<T>` jest rzutowaniem:
 *  lista agentów, którym import dopisał połączenia, ma wrócić listą albo niczym — nigdy
 *  wartością, na której składanie zdania rzuci wyjątkiem i zdejmie ekran. */
export async function applySetup(
  request: ApplyRequest,
  folder: string | null = activeWorkspace()?.folder ?? null,
): Promise<ImportReceipt> {
  const receipt = await invoke<ImportReceipt>('apply_setup', { request, folder });
  return { ...receipt, filledAgents: filledAgents(receipt.filledAgents) };
}

/** Jedno pytanie do agenta o kopie JEDNEJ pozycji. `null` znaczy „człowiek nacisnął Stop".
 *
 * Trzy klucze wypisane osobno, a nie `ask` podany w całości: `checks/invoke-args.sh` sądzi
 * wyłącznie literał obiektu o statycznych kluczach, a wywołanie podające zmienną jest przez
 * to sprawdzenie POMIJANE — czyli dokładnie ta krawędź, na której literówka w nazwie pola
 * odzywa się dopiero odmową pod palcem człowieka.
 */
export function compareCopies(
  ask: CompareRequest,
  folder: string | null = activeWorkspace()?.folder ?? null,
): Promise<Comparison | null> {
  return invoke<Comparison | null>('compare_import_copies', {
    folder,
    workspace: ask.workspace,
    item: ask.item,
    agent: ask.agent,
  });
}

/** „Stop" dla porównania, które trwa. Osobne od Stopu biegu i od Stopu draftu. */
export function stopComparing(): Promise<void> {
  return invoke<void>('stop_comparing_copies');
}

export function forProject(folder: string | null) {
  return {
    scanSetup: (workspace: string) => scanSetup(workspace, folder),
    stopComparing,
    applySetup: (request: ApplyRequest) => applySetup(request, folder),
    compareCopies: (ask: CompareRequest) => compareCopies(ask, folder),
  };
}
