/* Jedyna droga, którą gałąź wyniku wchodzi do aplikacji.
 *
 * Nazwy argumentów muszą co do znaku zgadzać się z parametrami komend w `ipc.rs`: Tauri
 * dopasowuje je PO NAZWIE i odrzuca całe wywołanie przy literówce, bez słowa na ekranie.
 */
import { invoke } from '@tauri-apps/api/core';

import { activeWorkspace } from '../../../state/workspaces';

/** Co ekran ma pokazać pod polem z identyfikatorem zadania. */
export interface Proposed {
  /** Nazwa, którą Loadout założy. Pusta, kiedy nie ma jeszcze identyfikatora. */
  readonly name: string;
  /** Przedrostek zmierzony z gałęzi tego repozytorium — albo `null`, i wtedy nie ma konwencji. */
  readonly convention: string | null;
  /** Czy taka gałąź już jest. */
  readonly taken: boolean;
}

/** Czym skończyło się składanie pracy biegu. */
export type Landing =
  | { readonly kind: 'landed'; readonly branch: string; readonly steps: number }
  | { readonly kind: 'nothing' }
  | { readonly kind: 'clash'; readonly with: string; readonly files: readonly string[] };

/**
 * Jak nazwie się gałąź dla tego identyfikatora.
 *
 * Cisza (pusta propozycja) przy braku workspace'u albo odmowie Rusta: to jest podpowiedź pod
 * polem, więc „nie wiem" jest poprawną odpowiedzią, a wyjątek zabiłby wpisywanie w połowie słowa.
 */
export async function proposedName(id: string): Promise<Proposed> {
  const folder = activeWorkspace()?.folder;
  if (folder === undefined || folder === '' || id.trim() === '') {
    return { name: '', convention: null, taken: false };
  }
  try {
    return await invoke<Proposed>('suggest_branch_name', { folder, id });
  } catch {
    return { name: '', convention: null, taken: false };
  }
}

/**
 * Składa pracę biegu pod podaną nazwą.
 *
 * Odmowa wraca ZDANIEM, nie ciszą: zajęta nazwa i zderzenie dwóch kroków to rzeczy, które
 * człowiek ma przeczytać i rozstrzygnąć, a nie awarie do przemilczenia.
 */
export async function foldRun(run: string, name: string): Promise<Landing | string> {
  const folder = activeWorkspace()?.folder;
  if (folder === undefined || folder === '') {
    return 'Pick a workspace first — Loadout needs to know which repository to write to.';
  }
  try {
    return await invoke<Landing>('fold_run_into_branch', { folder, run, name });
  } catch (trouble) {
    return typeof trouble === 'string' ? trouble : String(trouble);
  }
}
