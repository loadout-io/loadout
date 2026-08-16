/* AC-7 dla T-22 — zapadka może tylko maleć.
 *
 * Niezmiennik 18 mówi "baseline może tylko maleć" i łamie się cicho przez zapadkę, która
 * przy KAŻDYM pomiarze zapisuje aktualną wartość: skrypt biegnie, plik się zmienia, nic
 * nigdy nie jest czerwone. poprzedni prototyp ustawił swój próg po fakcie i zamarzł na 29 regionach
 * przy limicie 12 — 2,4× wartości docelowej [03 §4.1]. Zapadka ustawiona po fakcie jest
 * zawsze ustawiona tam, gdzie akurat jesteś.
 *
 * Sufit i zapadka to DWIE RÓŻNE ODMOWY i mają się czytać inaczej. "Przekroczyłeś limit
 * z ARCHITECTURE §7" znaczy "to nie wejdzie do produktu". "Jesteś powyżej ostatniego
 * pomiaru, choć pod limitem" znaczy "cofnąłeś się, napraw albo uzasadnij". Zlanie ich
 * w jedno zdanie kasuje całą wartość zapadki.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(code).not.toBe(0)` przy próbie podniesienia.
 * Przechodzi ją implementacja, która odmawia i MIMO TO zapisuje plik — czyli dokładnie ta,
 * przed którą to kryterium broni. Rozstrzygają dwie połowy naraz: porównanie BAJTÓW pliku
 * przed i po odmowie ORAZ dowód, że dozwolone obniżenie faktycznie plik zmieniło. Bez
 * drugiej połowy przechodzi też skrypt, który nie zapisuje nigdy.
 *
 * Niezmiennik 21: `checks/density-baseline.json` ma być CZYTANY przy każdym biegu, nie tylko
 * zapisywany przez `--update-baseline`. Plik zapisywany i nieczytany to `design/<task>/
 * plan.json` z repo źródłowego — artefakt, którego nikt nigdy nie otworzył.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  CEILING_FIXTURE,
  FIXTURES,
  copyCheck,
  copyScript,
  mustExist,
  plant,
  runCheck,
  sandbox,
} from './_support';

const CHECK = 'quick-density.sh';
const BASELINE_PATH = 'checks/density-baseline.json';

/** Zapadka z JEDNĄ metryką: chrome stoi na 80, przy suficie tej metryki równym 96. */
const BASELINE = `${JSON.stringify({ chromePixels: 80 }, null, 2)}\n`;

let dir = '';

/*
 * Piaskownica budowana LENIWIE, w środku testu, a nie w `beforeAll` — i to nie jest kwestia
 * stylu. `beforeAll`, który rzuca, zamienia wszystkie testy bloku w POMINIĘTE, a vitest
 * drukuje wtedy podsumowanie `Tests N skipped (N)`. Ten dokładny podpis stoi na liście
 * `NOT_A_REAL_RED` w harness/gate.py, więc bramka odrzuciłaby uczciwą czerwień tego
 * kryterium jako "did not RUN — the check itself is missing or broken". Zmierzone na tym
 * pliku: z setupem w `beforeAll` wychodziło "Tests 5 skipped (5)", czyli czerwień, która
 * nic nie poświadcza. Wywołane z wnętrza testu to samo rzucenie jest zwykłą porażką asercji.
 *
 * Przy okazji przywraca zapadkę do stanu wyjściowego przed każdym przypadkiem — testy
 * zapisujące plik nie mogą sobie nawzajem podmieniać punktu startowego.
 */
function tree(): string {
  if (dir === '') {
    const built = sandbox('density-ratchet');
    copyCheck(built, CHECK);
    // Sędzia, którego to sprawdzenie woła — kopiowany razem z nim, tak jak
    // `checks/_cargo-serialize.sh` w kryteriach wołających cargo.
    copyScript(built, 'density-audit.mjs');
    const doc = mustExist('docs/ARCHITECTURE.md', 'the only source of the seven density numbers');
    plant(built, 'docs/ARCHITECTURE.md', readFileSync(doc, 'utf8'));
    plant(built, 'src/main.tsx', 'export const nothing = 0;\n');
    dir = built;
  }
  plant(dir, BASELINE_PATH, BASELINE);
  return dir;
}

function withSnapshot(name: string): Record<string, string> {
  return { LOADOUT_DENSITY_SNAPSHOT: join(FIXTURES, name) };
}

/** Surowe bajty zapadki. Porównanie tekstu przespałoby przepisanie pliku z inną końcówką linii. */
function bytes(): Buffer {
  return readFileSync(join(dir, BASELINE_PATH));
}

function written(): Record<string, number> {
  return JSON.parse(readFileSync(join(dir, BASELINE_PATH), 'utf8')) as Record<string, number>;
}

describe('the ratchet refuses to move up', () => {
  it('refuses a measurement above the baseline even though it is below the ceiling', () => {
    // 90 mieści się pod sufitem 96, więc to NIE jest odmowa "za gęsto" — i komunikat musi
    // to powiedzieć, inaczej człowiek szuka regionu, którego nie ma za dużo.
    const run = runCheck(tree(), CHECK, [], withSnapshot('above-ratchet.json'));

    expect(run.code, run.out).toBe(1);
    expect(run.out).toMatch(/baseline/i);
    expect(run.out).toMatch(/shrink|lower|down/i);
    expect(run.out).toContain('chromePixels');
  });

  it('accepts a metric that the baseline file has never seen', () => {
    // Zapadka zna wyłącznie chromePixels. Sześć pozostałych metryk jest pierwszym pomiarem,
    // a pierwszy pomiar zawsze wolno przyjąć — inaczej nowej metryki nie da się włączyć.
    const run = runCheck(tree(), CHECK, [], withSnapshot('measured-zero.json'));

    expect(run.code, run.out).toBe(0);
  });
});

describe('--update-baseline writes in one direction only', () => {
  it('rewrites the file when the measurement went down', () => {
    const before = readFileSync(join(tree(), BASELINE_PATH));

    const run = runCheck(dir, CHECK, ['--update-baseline'], withSnapshot('below-ratchet.json'));

    expect(run.code, run.out).toBe(0);
    // Połowa druga kryterium: bez niej przechodzi skrypt, który nie zapisuje NIGDY.
    expect(bytes().equals(before), 'the allowed lowering left the baseline untouched').toBe(false);
    expect(written()['chromePixels']).toBe(70);
  });

  it('refuses to raise the baseline and leaves the file byte for byte identical', () => {
    const before = readFileSync(join(tree(), BASELINE_PATH));

    const run = runCheck(dir, CHECK, ['--update-baseline'], withSnapshot('above-ratchet.json'));

    expect(run.code, run.out).not.toBe(0);
    // Połowa pierwsza: odmowa, która mimo wszystko zapisała plik, zabetonowałaby regres
    // i wyglądałaby przy tym jak działająca obrona.
    expect(bytes().equals(before), 'the refused update still rewrote the baseline').toBe(true);
  });

  it('never writes a value that the ceiling does not allow', () => {
    const before = readFileSync(join(tree(), BASELINE_PATH));

    const run = runCheck(dir, CHECK, ['--update-baseline'], withSnapshot('one-over.json'));

    expect(run.code, run.out).not.toBe(0);
    expect(bytes().equals(before), 'an over-ceiling measurement reached the baseline').toBe(true);

    // I ta sama reguła jako własność PLIKU, nie jednej ścieżki kodu: zapadka nie ma prawa
    // zabetonować stanu gorszego niż stan deklarowany w ARCHITECTURE §7.
    for (const [metric, value] of Object.entries(written())) {
      const limit = CEILING_FIXTURE.find((entry) => entry.key === metric)?.limit;
      expect(limit, `the baseline holds a metric no ceiling row declares: ${metric}`).toBeDefined();
      expect(value).toBeLessThanOrEqual(limit ?? 0);
    }
  });
});
