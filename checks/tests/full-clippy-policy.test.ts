/* AC-3 dla T-22 — full-clippy.sh odmawia, kiedy polityka lintów nie jest podłączona.
 *
 * Polityka lintów mieszka w JEDNYM rdzeniu: `[workspace.lints]` w korzeniowym `Cargo.toml`
 * (niezmiennik 23). Członek workspace'u podłącza się do niej jedną linią `lints.workspace
 * = true`. Kiedy ta linia zniknie — albo, co gorsza, zamieni się w komentarz — clippy dalej
 * biegnie, dalej kończy zerem i dalej nie widzi ani jednego `unwrap()`. Bramka świeci
 * zielono z powodu, który nie ma nic wspólnego z jakością kodu.
 *
 * Zmierzone tutaj, w piaskownicy, przed napisaniem tej specyfikacji: z podłączoną polityką
 * `cargo clippy --all-targets -- -D warnings` odmawia na `.unwrap()` w celu testowym;
 * z polityką w komentarzu ten SAM kod przechodzi na czysto. To jest cała cicha awaria.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(stdout).toContain('workspace = true')` albo grep po
 * tym ciągu w `Cargo.toml`. Przechodzi **na komentarzu** — to jest dosłownie incydent
 * `--sandbox workspace-write` z raportu 06 §2, gdzie selftest asertował obecność flagi,
 * przechodził na komentarzu, a żywa flaga brzmiała `danger-full-access` (niezmiennik 20).
 * Rozstrzyga przypadek B: ciąg JEST w pliku obecny, a wymagany jest exit **2**.
 *
 * Przypadek B pilnuje przy okazji niezmiennika 23 od drugiej strony. Gdyby sprawdzenie
 * dokładało do wywołania własne `-D clippy::unwrap_used`, dostałoby w B exit 1 — czyli
 * zameldowałoby "twój kod jest zły" o drzewie, w którym zły jest NASZ plik konfiguracyjny,
 * i zamaskowałoby rozłączoną politykę. Dokładnie tak umarło po cichu skanowanie sekretów
 * na PR #535 [05 §4].
 *
 * Unwrap w fiksturze jest CELOWO nie-literalny. `Some(1).unwrap()` łapie domyślny
 * `clippy::unnecessary_literal_unwrap`, więc przypadek B padłby wtedy na clippy nawet
 * z rozłączoną polityką i niczego by nie rozróżniał. Zweryfikowane dwustopniowo.
 */
import { existsSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { copyCheck, lockPath, mustHaveCommand, plant, runCheck, sandbox } from './_support';

const CHECK = 'full-clippy.sh';
const MEMBER = 'src-tauri/Cargo.toml';
const TARGET = 'src-tauri/tests/x.rs';

const SLOW = 90_000;

const WORKSPACE = [
  '[workspace]',
  'resolver = "3"',
  'members = ["src-tauri"]',
  '',
  '[workspace.lints.clippy]',
  'unwrap_used = "deny"',
  '',
].join('\n');

const MEMBER_HEAD = [
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

/** Polityka podłączona — jedna linia, dokładnie jak w prawdziwym src-tauri/Cargo.toml. */
const CONNECTED = `${MEMBER_HEAD}[lints]\nworkspace = true\n`;

/** Polityka ROZŁĄCZONA, a ciąg "workspace = true" wciąż jest w pliku. To jest pułapka. */
const COMMENTED = `${MEMBER_HEAD}# lints.workspace = true\n`;

const LIB = 'pub fn one() -> u32 {\n    1\n}\n';

const TARGET_UNWRAPS = [
  'fn first(items: &[u32]) -> Option<u32> {',
  '    items.first().copied()',
  '}',
  '',
  '#[test]',
  'fn takes_the_first() {',
  '    let items = [7_u32, 8];',
  '    assert_eq!(first(&items).unwrap(), 7);',
  '}',
  '',
].join('\n');

const TARGET_CLEAN = [
  'fn first(items: &[u32]) -> Option<u32> {',
  '    items.first().copied()',
  '}',
  '',
  '#[test]',
  'fn takes_the_first() {',
  '    let items = [7_u32, 8];',
  '    assert_eq!(first(&items), Some(7));',
  '}',
  '',
].join('\n');

let dir = '';

beforeAll(() => {
  mustHaveCommand('cargo');
  dir = sandbox('full-clippy');
  copyCheck(dir, CHECK);
  copyCheck(dir, '_cargo-serialize.sh');
  plant(dir, 'Cargo.toml', WORKSPACE);
  plant(dir, 'src-tauri/src/lib.rs', LIB);
});

describe('full-clippy.sh separates a bad tree from a broken configuration', () => {
  it(
    'case A: connected policy bites unwrap() in a test target, which --lib never sees',
    () => {
      plant(dir, MEMBER, CONNECTED);
      plant(dir, TARGET, TARGET_UNWRAPS);

      const run = runCheck(dir, CHECK);

      expect(run.code, run.out).toBe(1);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );

  it(
    'case B: a commented-out lints.workspace is our broken configuration — exit 2, not 1',
    () => {
      plant(dir, MEMBER, COMMENTED);
      plant(dir, TARGET, TARGET_UNWRAPS);

      const run = runCheck(dir, CHECK);

      // 2, nie 1: zepsuł się NASZ plik konfiguracyjny, a nie sądzony kod. Exit 1 wysłałby
      // pisarza na polowanie po kodzie, w którym nie ma czego znaleźć.
      expect(run.code, run.out).toBe(2);
      expect(run.out).toMatch(/configurat|polic/i);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );

  it(
    'case C: connected policy and no unwrap() is a clean pass',
    () => {
      plant(dir, MEMBER, CONNECTED);
      plant(dir, TARGET, TARGET_CLEAN);

      const run = runCheck(dir, CHECK);

      expect(run.code, run.out).toBe(0);
      expect(existsSync(lockPath(dir)), 'the cargo lock outlived the check').toBe(false);
    },
    SLOW,
  );
});
