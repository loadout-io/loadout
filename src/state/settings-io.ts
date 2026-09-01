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
 * Trzy pola, bo trzy wybory. Przy liderze: wskazanie, nie opis agenta — vendor, model i dial
 * bezpieczeństwa czyta Rust z pliku definicji, a kopia któregokolwiek z nich trzymana obok
 * identyfikatora byłaby pierwszą rzeczą, która się rozjedzie (niezmiennik 13).
 */
export interface Settings {
  /** Identyfikator zapisanego agenta, albo `''`, dopóki nikt nie wybierał. */
  readonly defaultLead: string;
  /**
   * Ile wolno wydać na jeden bieg, dopóki człowiek nie wpisze innej kwoty na pasku Run.
   *
   * LICZBA, NIE `number | null`, i to jest cała treść tego pola. „Bez sufitu" nie jest tu
   * wyborem: Rust zawsze oddaje kwotę, a zdjęcie ograniczenia jest decyzją podejmowaną dla
   * JEDNEGO biegu, w pasku, i wtedy ekran mówi o tym na głos
   * (`sections/run/limits/budget.tsx`, `NO_CEILING_SAID`).
   */
  readonly defaultBudgetUsd: number;
  /**
   * Czy boczne menu stoi zwinięte do samych ikon.
   *
   * 2026-08-31 — TRZECI WYBÓR, bo trzeci raz to samo pytanie: „czy człowiek ma to wybierać przy
   * każdym uruchomieniu". Tryb nawigacji jest decyzją o tym, jak się pracuje, podejmowaną raz,
   * a nie czynnością przed pracą — dokładnie jak folder (2026-08-18) i lider (2026-08-29).
   *
   * `?`, bo plik zapisany przez wcześniejszą wersję Loadouta tego klucza nie ma, a `read_settings`
   * z atrapy granicy w kryteriach przeglądarkowych oddaje wyłącznie to, co scena wymieniła.
   * Brak klucza znaczy „nikt nie wybierał", nie „rozwinięte na siłę".
   */
  readonly navCollapsed?: boolean;
}

/**
 * Co stoi w pliku. **Pusty wybór jest poprawną odpowiedzią, nie błędem** — na świeżej maszynie
 * `~/.loadout/settings.json` jeszcze nie istnieje i Rust oddaje wtedy puste wskazanie.
 */
export function readSettings(): Promise<Settings> {
  return invoke<Settings>('read_settings');
}

/**
 * Zapisuje oba domyślne wybory i oddaje to, co ma teraz plik.
 *
 * Nazwy pól są częścią kontraktu, nie ozdobą: Tauri dopasowuje argumenty `invoke` PO NAZWIE,
 * więc `{ defaultLead, defaultBudgetUsd }` musi odpowiadać parametrom `default_lead`
 * i `default_budget_usd` skorupy w `src-tauri/src/ipc.rs`. Podmiana klucza nie jest błędem
 * kompilacji po żadnej ze stron — jest wywołaniem ODRZUCONYM, o którym nikt się nie dowie.
 *
 * WSZYSTKIE POLA W KAŻDYM ZAPISIE, bo plik jest jeden. Wywołanie niosące sam sufit nadpisałoby
 * lidera tym, co akurat trzymało okno, i odwrotnie — a to jest ta klasa rozjazdu, którą
 * „dysk pierwszy" (`./settings.ts`) miał zamknąć. Od 2026-08-31 dotyczy to także trybu menu.
 */
export function saveSettings(args: {
  defaultLead: string;
  defaultBudgetUsd: number;
  navCollapsed: boolean;
}): Promise<Settings> {
  return invoke<Settings>('save_settings', args);
}
