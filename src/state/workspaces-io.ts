/* Jedyne miejsce w całym repo, które zna nazwy trzech komend o workspace'ach
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` w magazynie. Magazyn jest DYSK-PIERWSZY: stan zmienia
 * się dopiero po potwierdzeniu z dysku, a to zdanie da się w ogóle wypowiedzieć tylko wtedy,
 * kiedy istnieje JEDNA krawędź, przez którą jedzie zapis. Dwie drogi do Rusta znaczą, że kolejność
 * pilnuje jednej z nich, a druga zmienia stan po cichu — i to jest defekt, który już raz w tym
 * repo wystąpił: agent zniknięty z listy przy NIEUDANYM usunięciu wracał po restarcie, bo okno
 * uwierzyło sobie, a nie plikowi.
 *
 * ZERO POLITYKI TUTAJ. Ani jednego `try`, ani jednego zdania dla człowieka, ani jednej domyślnej
 * wartości. Odmowa jedzie odrzuconą obietnicą do magazynu, bo to magazyn wie, czego właśnie
 * próbował, a `why()` (`src/ipc/why.ts`) wyjmie z niej zdanie, które napisał Rust. Tauri odrzuca
 * NAPISEM, nie `Error` — adapter, który by to opakowywał, kasowałby każdą precyzyjną odmowę.
 *
 * WSZYSTKIE TRZY ODDAJĄ CAŁĄ LISTĘ PO ZAPISIE, nie sam zapisany wpis
 * (`src-tauri/src/commands/workspaces.rs`): okno ma jedno źródło prawdy o liście i nie składa
 * jej sobie z odpowiedzi na pojedyncze zapisy. Lista złożona po stronie okna rozjeżdża się przy
 * pierwszym zapisie, który częściowo się nie udał.
 */
import { invoke } from '@tauri-apps/api/core';

import type { Workspace } from './workspaces';

/**
 * Workspace'y zapisane na dysku, w kolejności zapisu.
 *
 * **Pusta lista jest poprawną odpowiedzią, nie błędem.** Na świeżej maszynie pliku
 * `~/.loadout/workspaces.json` jeszcze nie ma i Rust oddaje wtedy `[]` — dokładnie ta pomyłka
 * (brakujący plik czytany jako awaria dysku) kończyła każdy bieg zdaniem „No such file or
 * directory (os error 2)".
 */
export function listWorkspaces(): Promise<Workspace[]> {
  return invoke<Workspace[]>('list_workspaces');
}

/**
 * Dokłada workspace albo ZMIENIA NAZWĘ istniejącego — klucz to folder.
 *
 * Nazwy pól są częścią kontraktu, nie ozdobą: Tauri dopasowuje argumenty `invoke` PO NAZWIE,
 * więc `{ name, folder }` musi brzmieć dokładnie tak, jak parametry skorupy w
 * `src-tauri/src/ipc.rs`. Podmiana klucza nie jest błędem kompilacji po żadnej ze stron.
 */
export function saveWorkspace(args: { name: string; folder: string }): Promise<Workspace[]> {
  return invoke<Workspace[]>('save_workspace', args);
}

/** Zdejmuje workspace z listy. **Folderu ani jego zawartości nie dotyka.** */
export function deleteWorkspace(args: { id: string }): Promise<Workspace[]> {
  return invoke<Workspace[]>('delete_workspace', args);
}
