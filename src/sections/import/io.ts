import { invoke } from '@tauri-apps/api/core';
import type {
  ApplyRequest,
  CompareRequest,
  Comparison,
  ImportPreview,
  ImportReceipt,
} from './setup';

export function scanSetup(workspace: string): Promise<ImportPreview> {
  return invoke<ImportPreview>('scan_setup', { workspace });
}

export function applySetup(request: ApplyRequest): Promise<ImportReceipt> {
  return invoke<ImportReceipt>('apply_setup', { request });
}

/** Jedno pytanie do agenta o kopie JEDNEJ pozycji. `null` znaczy „człowiek nacisnął Stop".
 *
 * Trzy klucze wypisane osobno, a nie `ask` podany w całości: `checks/invoke-args.sh` sądzi
 * wyłącznie literał obiektu o statycznych kluczach, a wywołanie podające zmienną jest przez
 * to sprawdzenie POMIJANE — czyli dokładnie ta krawędź, na której literówka w nazwie pola
 * odzywa się dopiero odmową pod palcem człowieka.
 */
export function compareCopies(ask: CompareRequest): Promise<Comparison | null> {
  return invoke<Comparison | null>('compare_import_copies', {
    workspace: ask.workspace,
    item: ask.item,
    agent: ask.agent,
  });
}

/** „Stop" dla porównania, które trwa. Osobne od Stopu biegu i od Stopu draftu. */
export function stopComparing(): Promise<void> {
  return invoke<void>('stop_comparing_copies');
}
