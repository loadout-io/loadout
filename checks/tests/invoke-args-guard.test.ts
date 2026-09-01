/* AC-8 dla T-38 — `checks/invoke-args.sh` UMIE ŚWIECIĆ NA CZERWONO, a nie tylko istnieje.
 *
 * DLACZEGO TO KRYTERIUM W OGÓLE JEST. 2026-08-17 okno wołało `run_workflow` z dwoma kluczami
 * z trzech. Tauri dopasowuje argumenty PO NAZWIE i deserializuje je, ZANIM wejdzie w ciało
 * komendy (tauri-macros 2.6.3 `wrapper.rs:505-507` zamienia ident na `lowerCamelCase`;
 * tauri 2.11.5 `ipc/channel.rs:300` pokazuje, że `Channel` NIE jest wstrzykiwany i czyta
 * stringa spod swojego klucza) — więc Start był odrzucany przy KAŻDYM kliknięciu. Kryterium,
 * które tego broniło, było zielone, bo rzutowało ładunek na DWA ręcznie wpisane klucze
 * i strukturalnie nie widziało brakującego trzeciego. Niezmiennik 28: reguła, która była
 * promptem i zawiodła mimo poprawnego brzmienia, ma się stać sprawdzeniem.
 *
 * SŁABA WERSJA TEGO KRYTERIUM, i jest nią dokładnie to, co kusi najbardziej: uruchomić
 * `invoke-args.sh` na prawdziwym drzewie i sprawdzić, że oddaje zero. Zielone byłoby
 * wtedy także sprawdzenie, które nie robi NIC — a takim właśnie kształtem commit kontraktowy
 * stawia ten plik. Odróżnia je to, że tutaj każdy przypadek stoi na ZASADZONYM naruszeniu
 * i wymaga kodu niezerowego oraz nazwy komendy i konkretnego klucza w wyjściu (niezmiennik 20).
 *
 * DRUGA SŁABA WERSJA, subtelniejsza: „przypadek poprawny oddał zero". Zero oddaje też skrypt,
 * który nie znalazł ani jednego wywołania — cisza i czystość wyglądają identycznie. Dlatego
 * przypadek (d) czyta z podsumowania LICZBĘ osądzonych wywołań i porównuje ją z liczbą
 * wywołań w zakresie, którą fikstura sama deklaruje. „0 passed" nie jest zielenią i tu też nie.
 *
 * FIKSTURA JEST HERMETYCZNA I TO JEST DECYZJA, NIE WYGODA. Prawdziwe `src-tauri/src/ipc.rs`
 * sądzi kryterium AC-1 tego samego zadania, w tym samym biegu bramki; gdyby sądziło je jeszcze
 * i to kryterium, czerwień jednego pasa przewracałaby drugi i żaden nie mówiłby, co jest zepsute.
 * Fikstura jest za to MODELOWANA na żywej sygnaturze `run_workflow` — z wstrzykiwanym `State`,
 * ze snake_case do zamiany i z `Channel`, który wstrzykiwany nie jest — bo sprawdzenie, które
 * przechodzi tylko na wymyślonych kształtach, nie broni niczego.
 *
 * Wartości oczekiwane: klucze na drucie NIE są przepisywane dwa razy. Jedna tablica `PARAMS`
 * renderuje i sygnaturę rustową w fiksturze, i literał obiektu w wywołaniu, więc te dwie strony
 * nie mają jak się rozjechać po cichu. Po zasadzeniu fikstura jest ODCZYTYWANA Z DYSKU i każdy
 * fragment sygnatury musi się w niej znaleźć — plik, który nie dojechał, ma paść tutaj, a nie
 * dwa przypadki dalej na „skrypt nic nie zgłosił".
 */
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import { REPO, copyCheck, plant, runCheck, sandbox } from './_support';

const CHECK = 'invoke-args.sh';
const IPC = 'src-tauri/src/ipc.rs';
const FRONT = 'src/sections/run/io.ts';
const ASIDE = 'src/sections/memory/io.ts';

/**
 * Parametry modelowanego `run_workflow`. `wire: null` znaczy „Tauri to wstrzykuje", czyli
 * żaden klucz na drucie mu nie odpowiada. Reszta to snake_case z Rusta i jego jedyna poprawna
 * postać po stronie okna — ta zamiana jest tym, czego to kryterium pilnuje.
 */
const PARAMS: ReadonlyArray<{ rust: string; wire: string | null }> = [
  { rust: "state: State<'_, AppState>", wire: null },
  { rust: 'file_name: &str', wire: 'fileName' },
  { rust: 'how_many_at_once: usize', wire: 'howManyAtOnce' },
  { rust: 'lines: Channel<Vec<Line>>', wire: 'lines' },
];

/** Klucze, których poprawne wywołanie MUSI nieść — wyprowadzone, nie przepisane. */
const WIRE: readonly string[] = PARAMS.map((p) => p.wire).filter((w): w is string => w !== null);

const IPC_FIXTURE = [
  '// Fikstura AC-8. Modelowana na zywej sygnaturze run_workflow: wstrzykiwany State,',
  '// snake_case do zamiany i Channel, ktory wstrzykiwany NIE jest.',
  'use tauri::State;',
  '',
  '#[tauri::command]',
  'pub async fn run_workflow(',
  ...PARAMS.map((p) => `    ${p.rust},`),
  ') -> Result<(), String> {',
  '    Ok(())',
  '}',
  '',
  '/// Komenda bez ani jednego klucza na drucie — sam wstrzykiwany stan.',
  '#[tauri::command]',
  "pub async fn stop_run(state: State<'_, AppState>) -> Result<(), String> {",
  '    Ok(())',
  '}',
  '',
].join('\n');

/** Wywołanie Startu z podanym zbiorem kluczy. Wartości nieistotne — sądzone są NAZWY. */
function startCall(keys: readonly string[]): string {
  const body = keys.map((k) => `    ${k}: whatever,`).join('\n');
  return [
    "import { invoke } from '@tauri-apps/api/core';",
    '',
    'const whatever = 1;',
    '',
    'export function start(): Promise<void> {',
    "  return invoke<void>('run_workflow', {",
    body,
    '  });',
    '}',
    '',
    'export function stop(): Promise<void> {',
    "  return invoke<void>('stop_run');",
    '}',
    '',
  ].join('\n');
}

/**
 * Wywołanie z argumentami w ZMIENNEJ. Poza zakresem sprawdzenia z premedytacją: grep nie
 * odpowiada na pytanie, co jest w `args`. Stoi tu po to, żeby przypadek (d) dowodził, że
 * „poza zakresem" znaczy „przemilczane", a nie „zgłoszone jako naruszenie".
 */
const ASIDE_FIXTURE = [
  "import { invoke } from '@tauri-apps/api/core';",
  '',
  'export function putToUse(args: { id: string }): Promise<void> {',
  "  return invoke<void>('put_note_to_use', args);",
  '}',
  '',
].join('\n');

/** Ile wywołań fikstura oddaje sprawdzeniu do osądzenia: Start i Stop. `putToUse` nie liczy się. */
const IN_SCOPE = 2;

let dir = '';

/** Czytanie, które pada na asercji o treści, a nie na otwarciu pliku (AGENTS.md §2a p. 5). */
function slurp(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

beforeAll(() => {
  dir = sandbox('invoke-args');
  copyCheck(dir, CHECK);
  plant(dir, IPC, IPC_FIXTURE);
  plant(dir, ASIDE, ASIDE_FIXTURE);
});

describe('invoke-args.sh compares invoke() keys against the signatures in ipc.rs', () => {
  it('the planted fixture really carries the signature these cases are judged against', () => {
    const written = slurp(join(dir, IPC));

    expect(
      written.length,
      'the fixture ipc.rs never reached disk, so every case below would judge an empty oracle',
    ).toBeGreaterThan(0);
    for (const param of PARAMS) {
      expect(
        written,
        `the fixture ipc.rs lost the parameter "${param.rust}", so the key set under test is not ` +
          'the one this criterion means to judge',
      ).toContain(param.rust);
    }
    expect(
      WIRE.length,
      'a run_workflow modelled with fewer than three wire keys cannot show a missing third key, ' +
        'which is the entire defect this check exists for',
    ).toBeGreaterThanOrEqual(3);
    expect(
      WIRE,
      'the channel key is what Tauri deserializes before entering the command body, so a fixture ' +
        'without it does not model the incident',
    ).toContain('lines');
  });

  it('case a: a call that DROPS a required key is refused, by command and by key name', () => {
    const dropped = 'lines';
    plant(dir, FRONT, startCall(WIRE.filter((k) => k !== dropped)));

    const run = runCheck(dir, CHECK);

    expect(
      run.code,
      'a call missing a required argument key is rejected by Tauri on every single click, ' +
        `so the check must not exit 0 on it. Output was:\n${run.out}`,
    ).not.toBe(0);
    expect(
      run.out,
      'the report has to name the command, or the writer cannot tell which seam is torn',
    ).toContain('run_workflow');
    expect(
      run.out,
      `the report has to name the missing key "${dropped}", or it says "something is wrong" ` +
        'and leaves the writer to grep for it',
    ).toContain(dropped);
  });

  it('case b: a key spelled snake_case instead of camelCase is refused', () => {
    plant(dir, FRONT, startCall(WIRE.map((k) => (k === 'fileName' ? 'file_name' : k))));

    const run = runCheck(dir, CHECK);

    expect(
      run.code,
      'Tauri renames command arguments to lowerCamelCase by default (tauri-macros wrapper.rs), ' +
        `so file_name never reaches the command and the check must refuse. Output was:\n${run.out}`,
    ).not.toBe(0);
    expect(run.out, 'the report has to name the command').toContain('run_workflow');
    expect(
      run.out,
      'the report has to name the key the window actually sent, or the writer cannot see the typo',
    ).toContain('file_name');
    expect(
      run.out,
      'the report has to name the key Rust actually expects, or the writer has to guess the fix',
    ).toContain('fileName');
  });

  it('case c: a key the command does not take is refused', () => {
    const surplus = 'howManyAtOnceReally';
    plant(dir, FRONT, startCall([...WIRE, surplus]));

    const run = runCheck(dir, CHECK);

    expect(
      run.code,
      'a surplus key is the visible half of a rename that only landed on one side of the seam, ' +
        `so silence here would hide the next drift. Output was:\n${run.out}`,
    ).not.toBe(0);
    expect(run.out, 'the report has to name the command').toContain('run_workflow');
    expect(
      run.out,
      `the report has to name the surplus key "${surplus}", not merely say the sets differ`,
    ).toContain(surplus);
  });

  it('case d: the correct call passes silently — and says how much it actually judged', () => {
    plant(dir, FRONT, startCall(WIRE));

    const run = runCheck(dir, CHECK);

    expect(
      run.code,
      `a check that complains about a correct tree gets worked around, not fixed. Output was:\n${run.out}`,
    ).toBe(0);
    expect(
      run.out,
      'a call whose arguments live in a variable is out of scope on purpose, and out of scope ' +
        'has to mean silent, not reported',
    ).not.toMatch(/missing key|unknown key|no #\[tauri::command\]/i);

    // Zero oddaje tez skrypt, ktory nie znalazl ani jednego wywolania. Podsumowanie ma powiedziec,
    // ILE oszadzil — inaczej „czysto" i „nic nie sprawdzilem" wygladaja tak samo (niezmiennik 19).
    const counted = /invoke-args: (\d+) invoke call/.exec(run.out);
    expect(
      counted,
      `the check passed without reporting how many calls it compared, so this green is ` +
        `indistinguishable from a check that found nothing. Output was:\n${run.out}`,
    ).not.toBeNull();
    expect(
      Number(counted?.[1]),
      'the fixture plants exactly the calls this check is supposed to see; a lower number means ' +
        'the scanner silently skipped one and its green means nothing',
    ).toBe(IN_SCOPE);
  });

  it('case e: an ipc.rs with no commands at all is our broken setup — exit 2, never a clean pass', () => {
    plant(dir, IPC, '// nothing here declares a command\npub fn nothing() {}\n');
    plant(dir, FRONT, startCall(WIRE));

    const run = runCheck(dir, CHECK);

    // 2, nie 0 i nie 1: kazdy zbior kluczy porownany z pusta lista przechodzi na niczym, wiec
    // „czysto" byloby wtedy zdaniem o naszym parserze, nie o sadzonym kodzie.
    expect(
      run.code,
      `an oracle that parsed zero commands makes every comparison pass on nothing, which is the ` +
        `exact shape of a check that never lights up. Output was:\n${run.out}`,
    ).toBe(2);

    plant(dir, IPC, IPC_FIXTURE);
  });

  /* 2026-08-28: pin przeniesiony z `checks/MANIFEST` do `harness/checks.json`.
   *
   * Powod istnienia tej asercji jest bez zmian i jest blizna (N-13): check odkrywany po nazwie
   * pliku, ktory zostanie SKASOWANY, nie produkuje nic -- a bramka melduje czysty przebieg.
   * Zmierzone: usuniecie `checks/quick-permissions.sh` dalo „7 checks, 0 failed" i exit 0.
   *
   * Zmienil sie WYLACZNIE plik, w ktorym stoi pin. `checks.json` jest teraz jedynym miejscem,
   * z ktorego harness wie, ze ten check istnieje i kiedy go odpalic, a `checks_are_declared`
   * w `scripts/ci.sh` pilnuje rozjazdu W OBIE STRONY. */
  it('case f: harness/checks.json declares invoke-args, or nothing would run it at all', () => {
    const cfg = JSON.parse(slurp(join(REPO, '.loadout', 'h', 'checks.json')));
    const commands = [
      ...Object.entries(cfg.checks ?? {}),
      ...Object.entries(cfg.manual_only ?? {}),
    ]
      .filter(([id]) => !id.startsWith('_'))
      .map(([, spec]) => (spec as { cmd?: string }).cmd ?? '');

    expect(
      commands.length,
      'checks.json parsed to zero commands, so the assertion below would pass or fail on an ' +
        'empty list rather than on the pin it is supposed to read',
    ).toBeGreaterThan(0);
    expect(
      commands.some((cmd) => cmd.includes(`checks/${CHECK}`)),
      `no check in checks.json runs checks/${CHECK}, so the file exists and nothing ever ` +
        'executes it -- which reads exactly like a check that passes',
    ).toBe(true);
  });
});
