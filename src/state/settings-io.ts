/* Jedyne miejsce w całym repo, które zna nazwy dwóch komend o tym, co Loadout robi domyślnie
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` w magazynie. Magazyn (`./settings.ts`) jest
 * DYSK-PIERWSZY: stan zmienia się dopiero po potwierdzeniu z dysku, a to zdanie da się w ogóle
 * wypowiedzieć tylko wtedy, kiedy istnieje JEDNA krawędź, przez którą jedzie zapis. Ten sam
 * kształt i ten sam powód, co w `./workspaces-io.ts`.
 *
 * ZERO POLITYKI TUTAJ. Ani jednego `try`, ani jednego zdania dla człowieka, ani jednej wartości
 * domyślnej. Odmowa jedzie odrzuconą obietnicą do magazynu, bo to magazyn wie, czego właśnie
 * próbował, a `why()` (`src/ipc/why.ts`) wyjmie z niej zdanie, które napisał Rust.
 */
import { invoke } from '@tauri-apps/api/core';

/**
 * Co Loadout robi domyślnie — lustro `commands::settings::SettingsWire`.
 *
 * Jedno pole, bo jeden wybór. Wskazanie, nie opis agenta: vendor, model i dial bezpieczeństwa
 * czyta Rust z pliku definicji, a kopia któregokolwiek z nich trzymana obok identyfikatora
 * byłaby pierwszą rzeczą, która się rozjedzie (niezmiennik 13).
 */
export interface Settings {
  /** Identyfikator zapisanego agenta, albo `''`, dopóki nikt nie wybierał. */
  readonly defaultLead: string;
}

/**
 * Co stoi w pliku. **Pusty wybór jest poprawną odpowiedzią, nie błędem** — na świeżej maszynie
 * `~/.loadout/settings.json` jeszcze nie istnieje i Rust oddaje wtedy puste wskazanie.
 */
export function readSettings(): Promise<Settings> {
  return invoke<Settings>('read_settings');
}

/**
 * Zapisuje domyślnego lidera i oddaje to, co ma teraz plik.
 *
 * Nazwa pola jest częścią kontraktu, nie ozdobą: Tauri dopasowuje argumenty `invoke` PO NAZWIE,
 * więc `{ defaultLead }` musi odpowiadać parametrowi `default_lead` skorupy w
 * `src-tauri/src/ipc.rs`. Podmiana klucza nie jest błędem kompilacji po żadnej ze stron —
 * jest wywołaniem ODRZUCONYM, o którym nikt się nie dowie.
 */
export function saveSettings(args: { defaultLead: string }): Promise<Settings> {
  return invoke<Settings>('save_settings', args);
}
