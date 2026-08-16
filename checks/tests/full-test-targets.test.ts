/* AC-2 dla T-22 — full-test.sh uruchamia cele testów integracyjnych i nie zostawia zamka.
 *
 * `checks/full-test.sh` woła dziś `cargo test --lib`, a `--lib` NIGDY nie kompiluje
 * `src-tauri/tests/*.rs`. Tymczasem każde kryterium akceptacji w tym repo ma postać
 * `cargo test --test <cel>` i mieszka właśnie tam: pełna bramka nie odpala celów, na których
 * stoi cała wyrocznia projektu. Test integracyjny może być czerwony od tygodnia, a
 * `./verify.sh full` powie "zielono".
 *
 * Druga rzecz w tym samym pliku to zamek. `_cargo-serialize.sh` bierze muteks
 * w `${TMPDIR:-/tmp}/loadout-cargo.lock` (niezmiennik 26), a `full-test.sh` po suicie Rusta
 * uruchamia vitesta — czyli nas. Zamek trzymany przez cały front blokuje `full-clippy.sh`
 * bez powodu, więc po KAŻDYM wywołaniu ma go nie być.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(code).toBe(1)` w przypadku A. Przechodzi ją
 * implementacja, która woła `cargo test` i tylko przepisuje jego kod wyjścia, wciąż licząc
 * przejścia z jednego celu. Rozstrzyga przypadek B: przy dwóch przechodzących testach —
 * jednym w `--lib`, jednym w `tests/thing.rs` — wypisany licznik musi brzmieć **2**.
 * Jedno przejście dowodzi, że policzono tylko `--lib`.
 *
 * Piaskownica ma minimalny crate BEZ ZALEŻNOŚCI: zmierzone 1,5 s na zimno, więc kryterium
 * mieści się w suficie 20 s, który bramka daje sprawdzeniu w tierze `before`. Układ katalogów
 * jest lustrem repo (`src-tauri/src`, `src-tauri/tests`, workspace w korzeniu), bo skopiowane
 * sprawdzenie liczy ROOT z BASH_SOURCE i szuka dokładnie tych ścieżek — kopiujemy je zamiast
 * łatać skrypt o zmienne środowiskowe.
 */
import { existsSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { copyCheck, lockPath, mustHaveCommand, plant, runCheck, sandbox } from './_support';

const CHECK = 'full-test.sh';
const THING = 'src-tauri/tests/thing.rs';

/** Zimny build tego crate'a idzie sekundy, ale nie milisekundy — 5 s vitesta byłoby za ciasne. */
const SLOW = 90_000;

const WORKSPACE = '[workspace]\nresolver = "3"\nmembers = ["src-tauri"]\n';

const MANIFEST = [
  '[package]',
  'name = "sandbox"',
  'version = "0.0.0"',
  'edition = "2021"',
  'publish = false',
  '',
  '[lib]',
  'name = "sandbox_lib"',
  'path = "src/lib.rs"',
  '',
].join('\n');

const LIB = [
  'pub fn one() -> u32 {',
  '    1',
  '}',
  '',
  '#[test]',
  'fn the_library_has_a_passing_test() {',
  '    assert_eq!(one(), 1);',
  '}',
  '',
].join('\n');

const THING_PASSES = [
  '#[test]',
  'fn the_integration_target_runs() {',
  '    assert_eq!(2 + 2, 4);',
  '}',
  '',
].join('\n');

/**
 * Ten sam cel z DWOMA przechodzącymi testami — łącznie trzy z całego drzewa.
 *
 * Powód, dla którego to jest osobna fikstura, a nie podmiana poprzedniej: przy jednym teście
 * na cel poprawna odpowiedź (2) jest równa LICZBIE CELÓW, więc implementacja licząca cele
 * zamiast przejść przechodzi przypadek B i niczego to nie wykrywa. Trzy jest liczbą, której
 * nie da się wziąć znikąd poza sumą po celach.
 */
const THING_TWICE = [
  '#[test]',
  'fn the_integration_target_runs() {',
  '    assert_eq!(2 + 2, 4);',
  '}',
  '',
  '#[test]',
  'fn the_integration_target_runs_twice() {',
  '    assert_eq!(3 + 3, 6);',
  '}',
  '',
].join('\n');

const THING_FAILS = [
  '#[test]',
  'fn the_integration_target_runs() {',
  '    assert_eq!(2 + 2, 5);',
  '}',
  '',
].join('\n');

/** Cel istnieje, kompiluje się i nie ma w nim ANI JEDNEGO `#[test]`. */
const THING_EMPTY = ['pub fn helper() -> u32 {', '    7', '}', ''].join('\n');

/**
 * Cel, którego jedyny test jest ODROCZONY przez `#[ignore]`.
 *
 * Cargo raportuje z niego dokładnie `0 passed`, tak samo jak z celu pustego, a to są dwie
 * różne rzeczy i tylko jedna z nich jest awarią. `#[ignore]` stoi w pliku, widać go przy
 * czytaniu i ma własne kryterium wołające `-- --include-ignored`; tak wygląda pięć celów
 * supervisora w tym repo, bo odpalają prawdziwe procesy i idą minuty. Cel pusty nie
 * deklaruje niczego i nikt nigdy się o tym nie dowie.
 */
const THING_IGNORED = [
  '#[test]',
  '#[ignore = "odpala prawdziwy proces; wolane osobnym kryterium"]',
  'fn the_deferred_target_runs_elsewhere() {',
  '    assert_eq!(2 + 2, 4);',
  '}',
  '',
].join('\n');

let dir = '';

beforeAll(() => {
  mustHaveCommand('cargo');
  dir = sandbox('full-test');
  copyCheck(dir, CHECK);
  // Sprawdzenie woła cargo, więc kopiujemy też pomocnika zamka — inaczej `source` w kopii
  // sięgnąłby po plik, którego w piaskownicy nie ma.
  copyCheck(dir, '_cargo-serialize.sh');
  plant(dir, 'Cargo.toml', WORKSPACE);
  plant(dir, 'src-tauri/Cargo.toml', MANIFEST);
  plant(dir, 'src-tauri/src/lib.rs', LIB);
});

/**
 * Licznik przejść, tak jak czyta go człowiek i bramka: pierwsza liczba w linii podsumowania.
 * Niezmiennik 19 — zielone bez licznika jest czerwone.
 */
function reported(out: string): number {
  return Number(out.match(/\b(\d+)\b/)?.[1] ?? Number.NaN);
}

describe('full-test.sh runs the integration targets, not only --lib', () => {
  it(
    'case A: a failing tests/thing.rs makes the check refuse',
    () => {
      plant(dir, THING, THING_FAILS);

      const run = runCheck(dir, CHECK);

      expect(run.code, run.out).toBe(1);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );

  it(
    'case B: two passing tests are reported as two, not as one',
    () => {
      plant(dir, THING, THING_PASSES);

      const run = runCheck(dir, CHECK);

      expect(run.code, run.out).toBe(0);
      // TA asercja jest całym kryterium. Jedno przejście znaczy, że policzono wyłącznie
      // `--lib`, a `src-tauri/tests/thing.rs` nigdy się nie skompilował.
      expect(reported(run.out), run.out).toBe(2);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );

  it(
    'case B2: three passing tests are reported as three, so the counter is not the target count',
    () => {
      plant(dir, THING, THING_TWICE);

      const run = runCheck(dir, CHECK);

      expect(run.code, run.out).toBe(0);
      // Przypadek B sam w sobie da się przejść licząc CELE (dwa cele, dwa przejścia).
      // Tutaj cele są wciąż dwa, a przejścia trzy — obie liczby się rozjeżdżają i tylko
      // suma po celach daje poprawną odpowiedź.
      expect(reported(run.out), run.out).toBe(3);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );

  it(
    'case C: a target with zero #[test] is a refusal, not a green run',
    () => {
      plant(dir, THING, THING_EMPTY);

      const run = runCheck(dir, CHECK);

      expect(run.code, run.out).toBe(1);
      // Niezmiennik 19: cel, który istnieje i nie zameldował ani jednego przejścia, to nie
      // jest sukces — to jest filtr, cfg albo niezadeklarowany moduł.
      expect(run.out).toMatch(/no passing tests|zero/i);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );

  it(
    'case D: a target whose only test is #[ignore] is a declared deferral, not an empty target',
    () => {
      plant(dir, THING, THING_IGNORED);

      const run = runCheck(dir, CHECK);

      // Bez tego przypadku reguła z C jest zbyt szeroka i przewraca się na każdym celu
      // odroczonym przez `#[ignore]` — a te w tym repo mają własne kryteria wołające
      // `-- --include-ignored`. Odroczenie widoczne w pliku nie jest cichym zerem.
      expect(run.code, run.out).toBe(0);
      expect(reported(run.out), run.out).toBe(1);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );
});
