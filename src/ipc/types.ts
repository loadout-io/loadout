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

/** Czy jedna wartość z drutu ma kształt, którego lustro się po niej spodziewa. */
type Field = (value: unknown) => boolean;

const str: Field = (value) => typeof value === 'string';
/* `Number.isFinite`, nie samo `typeof`: `NaN` i `Infinity` są w JS liczbami, a w JSON-ie nie
 * istnieją — jeśli któreś tu dotarło, to nie przyszło z serde i nie jest tym, na co patrzymy. */
const num: Field = (value) => typeof value === 'number' && Number.isFinite(value);
const flag: Field = (value) => typeof value === 'boolean';
const strs: Field = (value) => Array.isArray(value) && value.every(str);
/* `Option<T>` z Rusta jedzie na drut jako `null`, nigdy jako brak klucza — i na tej różnicy
 * stoi całe odrzucanie mutantów niżej: `detailId: null` jest poprawne, a wiersz BEZ `detailId`
 * nie jest, choćby niósł `detail_id` z dokładnie tą samą wartością. */
const maybeNum: Field = (value) => value === null || num(value);

/** Cztery rodzaje niosą tylko tekst i mają dokładnie ten sam komplet pól. */
const SAYS: Readonly<Record<string, Field>> = { agent: str, text: str };

/**
 * Pola, jakie ma mieć wiersz danego rodzaju — komplet, bez `kind`.
 *
 * `Map`, nie zwykły obiekt, i to nie jest gust: `SHAPES['constructor']` na obiekcie oddaje
 * funkcję z prototypu zamiast `undefined`, więc `{"kind":"constructor"}` z drutu przeszedłby
 * jako rodzaj, którego nikt nigdy nie zadeklarował.
 */
const SHAPES: ReadonlyMap<string, Readonly<Record<string, Field>>> = new Map([
  ['run', SAYS],
  ['step', SAYS],
  ['agent', SAYS],
  ['thinking', { agent: str }],
  ['read', { agent: str, text: str, count: num, paths: strs, detailId: maybeNum }],
  ['search', { agent: str, text: str, count: num, paths: strs, detailId: maybeNum }],
  [
    'edit',
    {
      agent: str,
      text: str,
      count: num,
      paths: strs,
      added: num,
      removed: num,
      detailId: maybeNum,
    },
  ],
  ['ran', { agent: str, text: str, ok: flag, preview: str, detail: strs, detailId: maybeNum }],
  ['note', SAYS],
  ['asked', { agent: str, text: str, options: strs }],
  ['handoff', SAYS],
  ['memory', { agent: str, text: str, path: str }],
  ['problem', { agent: str, text: str, resetsAt: maybeNum }],
  ['done', { agent: str, text: str, turns: num, durationMs: num, costUsd: maybeNum }],
]);

/**
 * Sprawdza jeden wiersz z drutu i oddaje go typowanego — albo `null`.
 *
 * `null`, nigdy wyjątek. Vendorzy dokładają typy zdarzeń co tydzień i po cichu, a wywrócony
 * `onmessage` zabiera CAŁY widok, nie jedną linię (niezmiennik 5 w duchu, po stronie frontu).
 * Nieznany rodzaj jest więc porzucany, nie zgłaszany.
 *
 * Zestaw kluczy musi się zgadzać CO DO JEDNEGO, w obie strony. Sprawdzenie samych pól, których
 * lustro się spodziewa, przepuściłoby wiersz niosący i `detailId`, i `detail_id` — a to jest
 * dokładnie ten kształt, który powstaje, kiedy ktoś doda pole w Ruście i zapomni
 * o `rename_all_fields`. W meetnotes taki `started_at` położył cały widok, a sześć pierwszych
 * poprawek poszło w warstwę, w której był tylko objaw [00-SYNTHESIS §3].
 */
export function parseLine(value: unknown): Line | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return null;
  }
  const row = value as Record<string, unknown>;

  const kind = row['kind'];
  if (typeof kind !== 'string') {
    return null;
  }
  const shape = SHAPES.get(kind);
  if (shape === undefined) {
    return null;
  }

  const fields = Object.keys(shape);
  // Ani jednego klucza za dużo: `kind` plus dokładnie tyle pól, ile ma ten rodzaj.
  if (Object.keys(row).length !== fields.length + 1) {
    return null;
  }

  for (const field of fields) {
    // `Object.hasOwn`, nie `field in row` i nie `row[field] !== undefined`: pierwsze widzi
    // prototyp, drugie nie odróżnia braku klucza od `null` — a to jest cała różnica między
    // poprawnym `detailId: null` a mutantem, który przyszedł po rustowemu.
    if (!Object.hasOwn(row, field)) {
      return null;
    }
    const check = shape[field];
    if (check === undefined || !check(row[field])) {
      return null;
    }
  }

  // Rzutowanie, nie kopia: zestaw kluczy i typ każdej wartości są sprawdzone linijkę wyżej,
  // a paczka ma do 2000 wierszy — przepisywanie ich do świeżych obiektów kosztuje dokładnie
  // tyle, ile pompa po tamtej stronie granicy właśnie zaoszczędziła.
  return row as unknown as Line;
}
