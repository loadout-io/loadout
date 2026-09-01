/* AC-1 dla T-22 — boundary.sh widzi kod platformowy, który nie jest napisany
 * `#[cfg(windows)]`.
 *
 * Sprawdzacz, którego nikt nigdy nie widział na czerwono, jest nieprzetestowany. Dzisiejszy
 * `boundary.sh` pilnuje niezmiennika 3 gerpem po `#[cfg(windows|unix|target_os…)]`,
 * więc `use libc::SIGTERM;` przechodzi bez słowa — a `libc` jest w src-tauri/Cargo.toml
 * zależnością WYŁĄCZNIE uniksową (`[target.'cfg(unix)'.dependencies]`) i to jest dokładnie
 * ten kod platformowy, który zamienia port na Windows z gałęzi cfg w przepisanie.
 * Niezmiennika 1 pilnuje gerpem po słowie "tauri", więc `use crate::ipc::Line;` — zależność
 * od jedynego modułu, który zna Tauri (ARCHITECTURE §3) — też przechodzi. A każdy plik
 * o nazwie `fake.rs` jest dziś wyłączony ze WSZYSTKICH trzech reguł razem z plikami
 * testowymi, mimo że jest kodem kompilowanym do binarki, nie testem.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(run(planted).code).not.toBe(0)` dla każdego
 * zasadzenia. Przechodzi ją skrypt przepisany na jedną linię `exit 1`. Rozstrzygają trzy
 * rzeczy, wszystkie w tym samym pliku: przypadki CISZY (te same tokeny w supervisor.rs,
 * w komentarzu i w pliku testowym) muszą dać exit 0, czyste drzewo musi dać exit 0
 * z niezerowym licznikiem obejrzanych plików, a każdy komunikat odmowy musi nieść ŚCIEŻKĘ
 * naruszającego pliku — odmowa, która nie mówi gdzie, jest nie do naprawienia i uczy ludzi
 * ignorować sprawdzacz (niezmiennik 20).
 *
 * Sprawdzenie jest KOPIOWANE do piaskownicy, nie łatane zmiennymi środowiskowymi: ROOT
 * liczy się z BASH_SOURCE, więc kopia sądzi drzewo testu bez ani jednej zmiany w kodzie
 * produkcyjnym. Patrz `_support.ts`.
 */
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { copyCheck, plant, runCheck, sandbox, type Run } from './_support';

const CHECK = 'boundary.sh';

const DAG = 'src-tauri/src/engine/dag.rs';
const SUPERVISOR = 'src-tauri/src/engine/supervisor.rs';
const FAKE = 'src-tauri/src/engine/drivers/fake.rs';
const RECOVERY = 'src-tauri/src/recovery.rs';
const WRITER = 'src-tauri/src/store/writer.rs';
const HELPERS = 'src-tauri/src/engine/tests/helpers.rs';

/** Czyste drzewo: pięć plików wysyłanych do binarki plus jeden plik pomocniczy testów. */
const CLEAN: Record<string, string> = {
  [DAG]: '//! Graf kroków: cykle i stopnie wejściowe.\n\npub struct Dag {\n    pub nodes: Vec<String>,\n}\n',
  [SUPERVISOR]:
    '//! Grupy procesów i eskalacja zabijania. JEDYNY plik z kodem platformowym.\n\npub fn pgid() -> i32 {\n    0\n}\n',
  [FAKE]:
    '//! Deterministyczny sterownik. Kompilowany do binarki — to nie jest plik testowy.\n\npub fn next_line() -> String {\n    String::new()\n}\n',
  [RECOVERY]:
    '//! Wznowienie biegów po restarcie aplikacji.\n\npub fn resume() -> usize {\n    0\n}\n',
  [WRITER]:
    '//! Jedyny pisarz do SQLite (niezmiennik 2).\n\npub fn append(line: &str) -> usize {\n    line.len()\n}\n',
  [HELPERS]:
    '//! Pomocniki testów silnika. Nie jest częścią wysyłanego artefaktu.\n\npub fn fixture() -> u8 {\n    0\n}\n',
};

let dir = '';

/** Przywraca całe drzewo do stanu czystego. Każdy przypadek zasadza dokładnie jedną rzecz. */
function restore(): void {
  for (const [path, body] of Object.entries(CLEAN)) {
    plant(dir, path, body);
  }
}

/** Zasadza jeden plik i uruchamia kopię sprawdzenia. */
function withPlanted(path: string, body: string): Run {
  plant(dir, path, body);
  return runCheck(dir, CHECK);
}

beforeAll(() => {
  dir = sandbox('boundary');
  copyCheck(dir, CHECK);
});

beforeEach(() => {
  restore();
});

describe('boundary.sh refuses platform code that never says cfg(windows)', () => {
  it('sees `use libc::SIGTERM;` in engine/dag.rs and names the file and invariant 3', () => {
    const run = withPlanted(
      DAG,
      '//! Graf kroków.\nuse libc::SIGTERM;\n\npub fn signal() -> i32 {\n    SIGTERM\n}\n',
    );

    expect(run.code, run.out).toBe(1);
    expect(run.out).toContain(DAG);
    expect(run.out).toMatch(/invariant 3\b/);
  });

  it('sees `use std::os::unix::process::CommandExt;` in store/writer.rs', () => {
    const run = withPlanted(
      WRITER,
      '//! Jedyny pisarz.\nuse std::os::unix::process::CommandExt;\n\npub fn append(line: &str) -> usize {\n    line.len()\n}\n',
    );

    expect(run.code, run.out).toBe(1);
    expect(run.out).toContain(WRITER);
    expect(run.out).toMatch(/invariant 3\b/);
  });

  it('sees `if cfg!(unix)` in recovery.rs — the macro form, not the attribute form', () => {
    const run = withPlanted(
      RECOVERY,
      '//! Wznowienie biegów.\n\npub fn resume() -> usize {\n    if cfg!(unix) {}\n    0\n}\n',
    );

    expect(run.code, run.out).toBe(1);
    expect(run.out).toContain(RECOVERY);
    expect(run.out).toMatch(/invariant 3\b/);
  });
});

describe('boundary.sh refuses a Tauri dependency inside engine/', () => {
  it('stops excusing engine/drivers/fake.rs, which ships in the binary and is not a test', () => {
    const run = withPlanted(
      FAKE,
      '//! Deterministyczny sterownik.\nuse tauri::AppHandle;\n\npub fn handle(app: &AppHandle) -> usize {\n    0\n}\n',
    );

    expect(run.code, run.out).toBe(1);
    expect(run.out).toContain(FAKE);
    expect(run.out).toMatch(/invariant 1\b/);
  });

  it('sees `use crate::ipc::Line;` — a dependency on Tauri that never says the word', () => {
    const run = withPlanted(
      DAG,
      '//! Graf kroków.\nuse crate::ipc::Line;\n\npub fn first(lines: &[Line]) -> usize {\n    lines.len()\n}\n',
    );

    expect(run.code, run.out).toBe(1);
    expect(run.out).toContain(DAG);
    expect(run.out).toMatch(/invariant 1\b/);
  });
});

describe('boundary.sh stays silent where the boundary is not crossed', () => {
  it('lets all three platform tokens live inside engine/supervisor.rs', () => {
    const run = withPlanted(
      SUPERVISOR,
      '//! JEDYNY plik z kodem platformowym.\nuse libc::SIGTERM;\nuse std::os::unix::process::CommandExt;\n\npub fn kill_group() -> i32 {\n    if cfg!(unix) {}\n    SIGTERM\n}\n',
    );

    expect(run.code, run.out).toBe(0);
  });

  it('lets `libc` appear inside a line comment', () => {
    const run = withPlanted(
      DAG,
      '//! Graf kroków.\n// libc::kill nie ma prawa tu wejść — eskalacja mieszka w supervisor.rs.\n\npub fn len() -> usize {\n    0\n}\n',
    );

    expect(run.code, run.out).toBe(0);
  });

  it('lets the same imports live in engine/tests/helpers.rs, which never ships', () => {
    const run = withPlanted(
      HELPERS,
      '//! Pomocniki testów silnika.\nuse libc::SIGTERM;\nuse tauri::AppHandle;\nuse std::os::unix::process::CommandExt;\n\npub fn fixture() -> i32 {\n    SIGTERM\n}\n',
    );

    expect(run.code, run.out).toBe(0);
  });

  it('passes a clean tree and says how many files it actually looked at', () => {
    const run = runCheck(dir, CHECK);

    expect(run.code, run.out).toBe(0);
    // Niezmiennik 19: kod wyjścia to nie dowód. Zielone bez liczby obejrzanych plików jest
    // nie do odróżnienia od `exit 0` postawionego w pierwszej linii skryptu.
    const seen = Number(run.out.match(/\b(\d+)\b/)?.[1] ?? 0);
    expect(seen, run.out).toBeGreaterThan(0);
  });
});
