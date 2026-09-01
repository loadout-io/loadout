/* Jedyna droga, którą podpowiedzi ścieżek wchodzą do aplikacji.
 *
 * Nazwy argumentów muszą co do znaku zgadzać się z parametrami `suggest_paths` w `ipc.rs`: Tauri
 * dopasowuje je PO NAZWIE i odrzuca całe wywołanie przy literówce, bez słowa na ekranie.
 */
import { invoke } from '@tauri-apps/api/core';

import { activeWorkspace } from '../../state/workspaces';

/** Jedno miejsce, które człowiek może wybrać. */
export interface Suggestion {
  /** Ścieżka względem folderu projektu; katalog kończy się `/`. */
  readonly path: string;
  /** Czy da się w nie wejść. */
  readonly folder: boolean;
}

/**
 * Co leży pod `typed` w folderze `folder`.
 *
 * Odmowa Rusta (`..`, ścieżka bezwzględna, znikły folder) wraca tu jako PUSTA lista, nie jako
 * wyjątek: to jest podpowiedź w polu tekstowym, więc „nie mam nic do zaproponowania" jest
 * poprawną odpowiedzią, a wyrzucony błąd zabiłby wpisywanie w połowie słowa.
 */
export async function suggestPaths(typed: string): Promise<Suggestion[]> {
  const folder = activeWorkspace()?.folder;
  /* Bez wybranego workspace'u nie ma korzenia, więc nie ma czego podpowiadać. Cisza jest tu
   * poprawna: pole działa dalej, tylko lista się nie otwiera. */
  if (folder === undefined || folder === '') return [];
  try {
    return await invoke<Suggestion[]>('suggest_paths', { folder, typed });
  } catch {
    return [];
  }
}
