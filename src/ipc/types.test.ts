/* Kryterium 7 dla T-07: lustro w TypeScripcie przyjmuje ten sam plik i nie wywraca się na
 * nieznanym rodzaju.
 *
 * `line-wire.golden.json` to jeden plik i dwie strony granicy. Rustowa strona sprawdza, że
 * pompa wysyła DOKŁADNIE to, co w nim stoi; ta sprawdza, że front to przyjmuje. Dryf jednej
 * ze stron jest czerwony u niej — i to jest cały powód, dla którego złoty plik istnieje
 * zamiast dwóch list pól utrzymywanych osobno.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(parseLine(golden[0])).toBeTruthy()`. Przechodzi na
 * funkcji `parseLine = (x) => x`, która niczego nie sprawdza i wpuszcza `snake_case` prosto do
 * komponentów. Rozróżniają je dwie rzeczy:
 *
 *   1. ODRZUCENIE mutanta `snake_case` — dowodzi, że funkcja w ogóle patrzy na klucze.
 *      To jest błąd z meetnotes: brakujący `rename_all_fields` posłał `started_at` do frontu
 *      i położył cały widok [00-SYNTHESIS §3].
 *   2. PORZUCENIE bez wyjątku wiersza o nieznanym rodzaju — odróżnia „waliduję" od „wywalam
 *      się na nowości". Vendorzy dokładają typy zdarzeń co tydzień i po cichu, a wywrócony
 *      `onmessage` zabiera cały widok, nie jedną linię.
 */
import { describe, expect, it } from 'vitest';
import golden from './line-wire.golden.json';
import { parseLine, WIRE_KINDS } from './types';

/** Złoty plik jako zwykłe obiekty: tu chodzi o KLUCZE, więc typ z wnioskowania przeszkadza. */
const entries = golden as unknown as Array<Record<string, unknown>>;

/**
 * Ile rodzajów wiersza jest — CZYTANE z lustra, nie przepisane.
 *
 * 2026-08-18 — stało tu `const KINDS = 14`. Ta liczba żyła w czterech miejscach naraz (tutaj,
 * w `KINDS: [LineKind; 14]` po stronie Rusta, w prozie obu plików i w liczbie wpisów złotego
 * pliku), a rodzaj dołożony na drucie zapalał tylko jedno z nich. Teraz oczekiwanie pochodzi
 * z `WIRE_KINDS`, czyli z tego samego lustra, którego kompletność ten plik sprawdza — a jeśli
 * lustro zwróciłoby pustkę, dowodzi tego asercja niżej, zamiast porównywać dwa zera.
 */
const KINDS = WIRE_KINDS.length;

/** Klucz na drucie i jego wersja po rustowemu — czyli dokładnie ten mutant, który ma odpaść. */
const RENAMED: ReadonlyArray<readonly [string, string]> = [
  ['detailId', 'detail_id'],
  ['resetsAt', 'resets_at'],
  ['durationMs', 'duration_ms'],
  ['costUsd', 'cost_usd'],
];

/** Pierwszy wpis niosący dany klucz. */
function entryWith(key: string): Record<string, unknown> {
  const found = entries.find((entry) => key in entry);
  if (found === undefined) {
    throw new Error(`the golden file has no line carrying the key ${key}`);
  }
  return found;
}

/** Kopia wpisu z jednym kluczem przemianowanym. */
function renamed(
  entry: Record<string, unknown>,
  from: string,
  to: string,
): Record<string, unknown> {
  const copy: Record<string, unknown> = { ...entry };
  copy[to] = copy[from];
  delete copy[from];
  return copy;
}

describe('parseLine', () => {
  it('takes every line in the golden file and hands back the same fields', () => {
    expect(entries).toHaveLength(KINDS);
    for (const entry of entries) {
      expect(parseLine(entry)).toEqual(entry);
    }
  });

  it('refuses a line whose key came over written the Rust way', () => {
    for (const [wire, underscored] of RENAMED) {
      const mutant = renamed(entryWith(wire), wire, underscored);
      expect(parseLine(mutant)).toBeNull();
    }
  });

  it('drops a kind the mirror has never heard of, and does not throw', () => {
    expect(() => parseLine({ kind: 'quantum' })).not.toThrow();
    expect(parseLine({ kind: 'quantum' })).toBeNull();
  });
});
