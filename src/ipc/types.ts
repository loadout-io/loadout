/* Lustro typów `Line` z `src-tauri/src/engine/line.rs`, pisane ręcznie.
 *
 * Ręcznie, a nie generatorem, i to jest decyzja: generator to trzecia rzecz do zbudowania,
 * a jedyne, co tu naprawdę trzeba pilnować, to kształt na drucie — pilnuje go
 * `line-wire.golden.json`, czytany z OBU stron granicy. Rozjazd tej strony jest czerwony
 * w `types.test.ts`, rozjazd tamtej w `src-tauri/tests/ipc_line_wire_golden.rs`.
 *
 * Klucze są camelCase, bo `Line` ma `rename_all_fields = "camelCase"`. To jest jedyna rzecz,
 * która stoi między nami a błędem z meetnotes: bez niej `detail_id`, `duration_ms`, `cost_usd`
 * i `resets_at` jadą na front pod nazwami, których on nie zna, widok wywraca się na
 * `undefined`, a pierwsze sześć poprawek idzie w złą warstwę, bo objaw jest w widoku,
 * a przyczyna w `derive` [00-SYNTHESIS §3].
 *
 * STAN TEGO PLIKU: SZKIELET (2026-08-16). Typy są prawdziwe — bez nich test się nie skompiluje
 * i nie uruchomi niczego. Ciało `parseLine` rzuca, więc kryterium pada na ZACHOWANIU, w czasie
 * wykonania, a nie na wczytywaniu modułu (`AGENTS.md` §2a p. 5).
 */

/** Wiersz, który niesie tylko tekst. Cztery rodzaje mają dokładnie ten kształt. */
interface Says<K extends string> {
  kind: K;
  agent: string;
  text: string;
}

/** Jeden wiersz historii — jedyna rzecz, którą dostaje widok. */
export type Line =
  | Says<'run'>
  | Says<'step'>
  | Says<'agent'>
  | Says<'note'>
  | Says<'handoff'>
  | { kind: 'thinking'; agent: string }
  | {
      kind: 'read';
      agent: string;
      text: string;
      count: number;
      paths: string[];
      detailId: number | null;
    }
  | {
      kind: 'search';
      agent: string;
      text: string;
      count: number;
      paths: string[];
      detailId: number | null;
    }
  | {
      kind: 'edit';
      agent: string;
      text: string;
      count: number;
      paths: string[];
      added: number;
      removed: number;
      detailId: number | null;
    }
  | {
      kind: 'ran';
      agent: string;
      text: string;
      ok: boolean;
      preview: string;
      detail: string[];
      detailId: number | null;
    }
  | { kind: 'asked'; agent: string; text: string; options: string[] }
  | { kind: 'memory'; agent: string; text: string; path: string }
  | { kind: 'problem'; agent: string; text: string; resetsAt: number | null }
  | {
      kind: 'done';
      agent: string;
      text: string;
      turns: number;
      durationMs: number;
      costUsd: number | null;
    };

/**
 * Sprawdza jeden wiersz z drutu i oddaje go typowanego — albo `null`.
 *
 * `null`, nigdy wyjątek. Vendorzy dokładają typy zdarzeń co tydzień i po cichu, a wywrócony
 * `onmessage` zabiera CAŁY widok, nie jedną linię (niezmiennik 5 w duchu, po stronie frontu).
 * Nieznany rodzaj jest więc porzucany, nie zgłaszany.
 */
export function parseLine(value: unknown): Line | null {
  // SZKIELET (2026-08-16). Świadomie brak zachowania: funkcja rzuca, więc kryterium pada na
  // pierwszej asercji, w czasie wykonania. Stub zwracający `value` przechodziłby połowę
  // kryterium (czternaście wpisów wchodzi i wychodzi), a to jest dokładnie ta funkcja
  // `parseLine = (x) => x`, która wpuszcza `snake_case` prosto do komponentów.
  throw new Error(`not implemented: parseLine got ${typeof value}`);
}
