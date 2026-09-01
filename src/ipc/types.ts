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
 * a przyczyna w `derive` [FOUNDATIONS §3].
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
  /* NIE `Says`: proza niesie o jedno pole więcej i lustro porównuje zestaw kluczy CO DO JEDNEGO,
   * więc kształt pożyczony od pozostałych porzucałby każdy wiersz prozy w ciszy.
   *
   * `body` jest CAŁĄ prozą, kiedy nie mieści się w wierszu, i pustą listą, kiedy się mieści —
   * reguła 1 mówi „treść siedzi ZA wierszem, nigdy w nim", a to pole jest jedyną drogą, którą
   * proza może za ten wiersz trafić. Pełny powód stoi przy `Line::Note` po stronie Rusta. */
  | { kind: 'note'; agent: string; text: string; body: string[] }
  /* Tura CZLOWIEKA. Osobny rodzaj, nie `note`: `note` znaczy „to powiedzial agent”, a widok
   * rysuje te dwie rzeczy inaczej, bo czytelnik musi widziec, czyje zdanie czyta. Powod, dla
   * ktorego to w ogole jedzie drutem, stoi przy `Line::Told` po stronie Rusta. */
  | Says<'told'>
  /* Lider proponuje bieg: proza plus gotowa komenda. TRZY POLA, nie dwa, i to jest cały powód,
   * dla którego nie jest to `note` z ukośnikiem w środku: `text` przyjeżdża sklejony do jednej
   * linii (reguła 1), więc granica między komendą a powodem, dla którego lider ją podaje, jest
   * po tej stronie granicy nieodtwarzalna. Okno, które składa komendę z powrotem z prozy, jest
   * tym samym oknem, które samo szuka `/run` — tylko o jeden krok dalej (niezmiennik 15).
   * Rozstrzygnięcie i pełny powód stoją przy `Line::Suggested` po stronie Rusta. */
  | { kind: 'suggested'; agent: string; text: string; command: string; auto: boolean }
  | Says<'handoff'>
  | { kind: 'thinking'; agent: string }
  /* Stan kroku zmienił się. NIE jest wierszem historii — przestawia blok paska loadoutu
   * i chip na kafelku agenta, dokładnie tak, jak `thinking` nie wchodzi do historii,
   * a zajmuje stały slot na dole (`docs/ARCHITECTURE.md` §6, reguła 5).
   *
   * OSOBNY RODZAJ, a nie pole dopisane do `step`, i to jest wymuszone przez lustro niżej:
   * zestaw kluczy musi się zgadzać CO DO JEDNEGO, więc pole dołożone do istniejącego rodzaju
   * kazałoby froncie PORZUCAĆ każdy wiersz `step` do chwili, w której obie strony granicy
   * zmienią się w tym samym commicie. Nowy rodzaj jest addytywny w obie strony: starszy front
   * porzuca go w ciszy (jedna linia mniej), starszy Rust go po prostu nie wysyła. */
  | { kind: 'stepState'; agent: string; stepId: string; state: string }
  /* Cały wynik nieudanego kroku, po którym polityka kazała jechać dalej: odbiorca ustawia z
   * niego `failed` i kwalifikację jednym zapisem. Osobny addytywny rodzaj, nie pole `stepState`:
   * starsze lustro może porzucić jeden nieznany fakt, zamiast odrzucić każdą linię stanu o
   * zmienionym zestawie kluczy. */
  | { kind: 'stepCarriedOn'; agent: string; stepId: string }
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
      /* Trzy liczniki tej tury, przepisane z drutu. Vendor, który nie podaje kwoty, podaje
       * przynajmniej te — i to z nich pasek składa `12k tokens` tam, gdzie nie ma czego wyliczyć
       * w dolarach. Zero znaczy „nic nie zgłoszono", bo tyle właśnie niesie `Tokens` po tamtej
       * stronie; pustkę na ekranie rozstrzyga suma, nie brak pola. */
      inputTokens: number;
      outputTokens: number;
      cachedTokens: number;
      /** Jak się skończyło — lustro `engine::line::Ended`. Okno NIE czyta tego z `text`. */
      ended: 'well' | 'badly' | 'stopped';
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
  ['stepState', { agent: str, stepId: str, state: str }],
  ['stepCarriedOn', { agent: str, stepId: str }],
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
  ['note', { agent: str, text: str, body: strs }],
  ['told', SAYS],
  /* NIE `SAYS`: propozycja niesie o jedno pole więcej i lustro porównuje zestaw kluczy CO DO
   * JEDNEGO, więc kształt pożyczony od prozy porzucałby każdy taki wiersz w ciszy — widok
   * po prostu nigdy nie pokazywałby tego, co lider zaproponował. */
  /* `auto` jest tu obowiązkowe, nie opcjonalne, i to jest cała różnica między wierszem, który
   * uruchamia bieg, a wierszem, który go proponuje. Klucz nieobecny czytałby się jak `false`
   * przez przypadek — a lustro porównuje ZESTAW kluczy co do jednego, więc wiersz bez niego ma
   * zostać porzucony głośno, a nie zinterpretowany po cichu. */
  ['suggested', { agent: str, text: str, command: str, auto: flag }],
  ['asked', { agent: str, text: str, options: strs }],
  ['handoff', SAYS],
  ['memory', { agent: str, text: str, path: str }],
  ['problem', { agent: str, text: str, resetsAt: maybeNum }],
  [
    'done',
    {
      agent: str,
      text: str,
      turns: num,
      durationMs: num,
      costUsd: maybeNum,
      inputTokens: num,
      outputTokens: num,
      cachedTokens: num,
      ended: str,
    },
  ],
]);

/**
 * Rodzaje wiersza, jakie drut umie wyprodukować — nazwa w nazwę, w kolejności deklaracji.
 *
 * WYEKSPORTOWANE, ŻEBY NIE TRZEBA BYŁO ICH PRZEPISYWAĆ. Rejestr widoku
 * (`src/sections/run/feed/kinds.ts`) musi znać dokładnie te rodzaje i ani jednego więcej,
 * a do 2026-08-18 jego kryterium miało tę listę **wpisaną z palca** — czyli było jednym z 68
 * kryteriów, których wartość oczekiwana nie pochodzi z żadnego pliku. Rodzaj dodany po stronie
 * Rusta zapala teraz to kryterium sam, bez niczyjej pamięci.
 *
 * Nie jest to pytanie rejestru o zdanie na własny temat: to DRUGIE, niezależne źródło.
 * Rejestr pusty przestanie się zgadzać z tą listą, a lista pusta (gdyby ktoś ją zepsuł)
 * przestanie się zgadzać z rejestrem — w obie strony.
 */
export const WIRE_KINDS: readonly string[] = Object.freeze([...SHAPES.keys()]);

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
 * poprawek poszło w warstwę, w której był tylko objaw [FOUNDATIONS §3].
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
