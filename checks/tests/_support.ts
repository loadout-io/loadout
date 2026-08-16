/* Wspólna hydraulika siedmiu specyfikacji T-22. Nie jest testem — nazwa bez `.test.` jest
 * poza wzorcem zbierania vitesta, a podkreślenie z przodu to ta sama konwencja, co
 * `checks/_cargo-serialize.sh`: plik pomocniczy, nie sprawdzenie.
 *
 * Mieszkają tu "Trzy rzeczy, bez których te testy nie zadziałają" z TASK.md — w jednym
 * miejscu, zamiast przepisane siedem razy:
 *
 *   1. Sprawdzenie KOPIUJEMY do piaskownicy zamiast łatać je o zmienne środowiskowe.
 *      Każdy skrypt w checks/ wylicza ROOT z BASH_SOURCE, więc kopia w $scratch/checks/
 *      widzi $scratch jako korzeń repo i sądzi drzewo testu bez ani jednej zmiany
 *      w kodzie produkcyjnym — i bez dotykania prawdziwego drzewa.
 *
 *   2. TMPDIR wskazuje piaskownicę przy KAŻDYM uruchomieniu. `_cargo-serialize.sh` bierze
 *      zamek w `${TMPDIR:-/tmp}/loadout-cargo.lock`; bez podmiany kopia sięgnęłaby po ten
 *      sam zamek, który trzyma zewnętrzny `full-test.sh` — a on odpala vitesta, czyli nas.
 *      To jest zakleszczenie na 300 s (cap `LOADOUT_CARGO_LOCK_WAIT`), które czyta się jak
 *      losowy timeout. Niezmiennik 26.
 *
 *   3. Najpierw istnienie artefaktu, z WŁASNYM komunikatem, dopiero potem cokolwiek
 *      uruchamiamy. `No such file or directory`, `ENOENT` i `command not found` są na
 *      liście `NOT_A_REAL_RED` w harness/gate.py, więc brakujący plik daje w tierze
 *      `before` czerwień, którą bramka odrzuca ze zdaniem "did not RUN". `mustExist()`
 *      rzuca zdaniem, w którym nie ma ani jednego z tych podpisów, i nie odpala niczego.
 *
 * Piaskownice leżą w `.loadout/scratch/` WEWNĄTRZ worktree. Nie w `$TMPDIR`: ścieżki spoza
 * worktree bywają odmawiane przez sandbox nieprzewidywalnie (zmierzone na S-1), a
 * `.loadout/scratch` jest na liście ścieżek generowanych w checks/quick-scope.sh, więc
 * piaskownica nie czyta się jak zapis poza zakresem zadania.
 */
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Korzeń repo, wyliczony z położenia tego pliku — nie z cwd, które vitest może zmienić. */
export const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** Katalog fikstur zrzutów. Te same pliki karmią sędziego i szew skryptu. */
export const FIXTURES = join(REPO, 'checks', 'tests', 'fixtures');

/**
 * Bezwzględna ścieżka do artefaktu, który MUSI istnieć, zanim cokolwiek uruchomimy.
 *
 * Rzuca własnym zdaniem zamiast pozwolić `bash` wypisać "No such file or directory".
 * To nie jest ozdoba: tamten podpis jest na liście NOT_A_REAL_RED i zamienia czerwień
 * kryterium w "sprawdzenie się nie uruchomiło", czyli w rundę, która nic nie poświadcza.
 */
export function mustExist(relative: string, what: string): string {
  const path = join(REPO, relative);
  if (!existsSync(path)) {
    throw new Error(
      `${what} has not been written yet: ${relative} is absent from the tree, ` +
        'so this criterion has nothing to judge',
    );
  }
  return path;
}

/**
 * Narzędzie, bez którego kryterium nie ma czego sądzić — sprawdzone WŁASNYM zdaniem.
 *
 * Ta sama zasada, co `mustExist`: `command not found` jest na liście NOT_A_REAL_RED, więc
 * brak `cargo` bez tego sprawdzenia zamienia kryterium w czerwień, którą bramka odrzuca.
 */
export function mustHaveCommand(name: string): void {
  const probe = spawnSync(name, ['--version'], { encoding: 'utf8' });
  if (probe.error !== undefined || probe.status !== 0) {
    throw new Error(
      `this criterion runs ${name} in a sandbox tree, and ${name} does not answer --version here`,
    );
  }
}

/** Świeża, pusta piaskownica pod `.loadout/scratch/t22/<nazwa>`. Kasowana przy każdym biegu. */
export function sandbox(name: string): string {
  const dir = join(REPO, '.loadout', 'scratch', 't22', name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** Zasadza plik w piaskownicy, tworząc po drodze katalogi. */
export function plant(dir: string, relative: string, body: string): void {
  const target = join(dir, relative);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, body, 'utf8');
}

/** Usuwa zasadzony plik — kolejny przypadek startuje z czystego drzewa. */
export function uproot(dir: string, relative: string): void {
  rmSync(join(dir, relative), { force: true });
}

/**
 * Kopiuje sprawdzenie z `checks/` do `<piaskownica>/checks/`. Po tej jednej linii kopia
 * liczy ROOT jako piaskownicę i nie ma pojęcia, że prawdziwe repo istnieje.
 */
export function copyCheck(dir: string, name: string): void {
  const source = mustExist(join('checks', name), `the check this criterion judges (${name})`);
  mkdirSync(join(dir, 'checks'), { recursive: true });
  copyFileSync(source, join(dir, 'checks', name));
}

export interface Run {
  /** Kod wyjścia. -1 znaczy "zabity sygnałem", nigdy "przeszło". */
  code: number;
  /** stdout i stderr sklejone: komunikat odmowy bywa na jednym i na drugim. */
  out: string;
}

/**
 * Uruchamia kopię sprawdzenia w piaskownicy. TMPDIR ZAWSZE wskazuje piaskownicę —
 * jednolicie, bo pomyłka w tę stronę jest niewidoczna aż do zakleszczenia (patrz nagłówek).
 */
export function runCheck(
  dir: string,
  name: string,
  args: string[] = [],
  env: Record<string, string> = {},
): Run {
  const script = join(dir, 'checks', name);
  const done = spawnSync('bash', [script, ...args], {
    cwd: dir,
    encoding: 'utf8',
    env: {
      ...process.env,
      TMPDIR: dir,
      NO_COLOR: '1',
      CARGO_TERM_COLOR: 'never',
      ...env,
    },
    // Sufit własny, poniżej sufitu vitesta: proces, który wisi, ma wrócić z kodem, a nie
    // zabrać ze sobą cały plik specyfikacji.
    timeout: 120_000,
  });
  return {
    code: done.status ?? -1,
    out: `${done.stdout ?? ''}${done.stderr ?? ''}`,
  };
}

/** Zamek, którego `_cargo-serialize.sh` używa przy TMPDIR ustawionym na piaskownicę. */
export function lockPath(dir: string): string {
  return join(dir, 'loadout-cargo.lock');
}

export interface CeilingEntry {
  /** Klucz maszynowy — tym mówi zrzut i tym mówi sędzia. */
  key: string;
  /** Etykieta wiersza z docs/ARCHITECTURE.md §7, dosłownie. */
  label: string;
  limit: number;
}

/**
 * Sufit jako FIKSTURA — argument dla `judge()`, nie twierdzenie o dokumencie.
 *
 * Twierdzenie o dokumencie stoi w jednym miejscu, w AC-4, i tam siedem liczb jest wpisanych
 * wprost: to jest cała treść tamtego kryterium. Tutaj są tylko po to, żeby sędzia dostał
 * realistyczne wejście. Kolejność jest kolejnością wierszy tabeli §7 i to też jest część
 * kontraktu — `readCeiling()` ma zwracać tablicę, bo "siódmy wiersz" musi dać się nazwać.
 */
export const CEILING_FIXTURE: readonly CeilingEntry[] = [
  { key: 'labelledRegions', label: 'Oznaczone regiony na ekranie', limit: 8 },
  { key: 'chromePixels', label: 'Piksele chrome nad pierwszą treścią', limit: 96 },
  { key: 'textElements', label: 'Elementy niosące tekst w widoku domyślnym', limit: 60 },
  { key: 'liveRegionsPerFact', label: 'Żywe regiony na jeden fakt', limit: 1 },
  { key: 'agentCardLines', label: 'Linie tekstu w kafelku agenta', limit: 4 },
  { key: 'animatedRegions', label: 'Regiony animujące się od jednego zdarzenia', limit: 2 },
  { key: 'navigationAxes', label: 'Osie nawigacji na ekranie', limit: 2 },
];

/** Dwie mierzone szerokości okna: najwęższa wspierana (DESIGN.md §9) i szeroka [03 §4.1]. */
export const WIDTHS = [1100, 1512] as const;

export interface Snapshot {
  widths: Array<{ width: number; metrics: Record<string, number> }>;
  /** Metryka → powód, dla którego kolektor jej nie zmierzył. Nigdy zero zamiast powodu. */
  notMeasured?: Record<string, string>;
}

export interface OverEntry {
  metric: string;
  measured: number;
  limit: number;
}

export interface Verdict {
  verdict: string;
  over: OverEntry[];
  notMeasured: string[];
}
