/* AC-6 dla T-22 — metryka niezmierzona nigdy nie czyta się jak zero, a "nie dało się"
 * to inne wyjście niż "nie ma czego".
 *
 * To jest najbardziej podstępna z czterech cichych awarii tego zadania: metryka, której
 * kolektor nie zmierzył, zapisana jako `0` i porównana z sufitem 8. Zielono. Zawsze.
 * poprzedni prototyp opublikował "czysty przebieg axe", który nie zmierzył niczego [03 §4.1] —
 * i to jest dokładnie ta różnica, którą zgubił.
 *
 * Dwie warstwy, bo są to dwie różne pomyłki. SĘDZIA musi odróżnić brak klucza od klucza
 * o wartości zero; te dwie rzeczy w JSON-ie wyglądają podobnie i różnią się wszystkim.
 * SKRYPT musi odróżnić cztery stany świata czterema kodami wyjścia, bo kod wyjścia jest
 * jedyną rzeczą, którą bramka naprawdę czyta:
 *
 *     0  zmierzone i pod zapadką
 *     0  nie ma czego mierzyć — z NAZWANYM warunkiem, nigdy w milczeniu
 *     1  za gęsto albo powyżej zapadki
 *     2  NIE DAŁO SIĘ zmierzyć
 *
 * Zlanie `0 (nie ma czego)` z `2 (nie dało się)` jest awarią, która wygląda jak sukces
 * i utrzymuje się latami: bramka melduje zielono na maszynie, na której nic nie policzono.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(judge(missing).verdict).not.toBe('pass')`. Przechodzi
 * ją sędzia, który odrzuca wszystko. Rozstrzyga zrzut z JAWNYM ZEREM, który w tym samym
 * pliku musi dać `pass`, plus tabela wyjść skryptu, w której 0 i 2 są różne.
 *
 * Kolektor jest odseparowany od sędziego szwem `LOADOUT_DENSITY_SNAPSHOT`: skrypt dostaje
 * gotowy zrzut i nie musi startować przeglądarki. Bez tego szwu kryterium wymagałoby
 * Chromium, a "Failed to launch" i "Executable doesn't exist" są na liście NOT_A_REAL_RED.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { judge } from '../../scripts/density-audit.mjs';
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
import measuredZero from './fixtures/measured-zero.json';
import missingMetric from './fixtures/missing-metric.json';

const CHECK = 'density.sh';
const NO_RATCHET = {};

/** Zapadka, w której stoi JEDNA metryka. Pozostałe sześć jest pierwszym pomiarem. */
const BASELINE = `${JSON.stringify({ chromePixels: 96 }, null, 2)}\n`;

/** Snapshot podany szwem — ścieżka do gotowej fikstury zamiast startowania przeglądarki. */
function withSnapshot(name: string): Record<string, string> {
  return { LOADOUT_DENSITY_SNAPSHOT: join(FIXTURES, name) };
}

describe('the judge tells a measured zero apart from a metric nobody measured', () => {
  it('reports a missing key by name and refuses to call the run a pass', () => {
    const verdict = judge(missingMetric, CEILING_FIXTURE, NO_RATCHET);

    expect(verdict.notMeasured).toEqual(['agentCardLines']);
    expect(verdict.verdict).not.toBe('pass');
  });

  it('treats an explicit zero as measured, because zero lines is a real answer', () => {
    // TA asercja jest całym kryterium. Sędzia odrzucający wszystko przechodzi test wyżej
    // i pada tutaj; sędzia czytający `undefined` jako `0` pada wyżej i przechodzi tutaj.
    const verdict = judge(measuredZero, CEILING_FIXTURE, NO_RATCHET);

    expect(verdict.notMeasured).toEqual([]);
    expect(verdict.verdict).toBe('pass');
  });
});

describe('density.sh answers with four different exit codes', () => {
  let dir = '';

  /*
   * Piaskownica budowana LENIWIE, w środku testu, a nie w `beforeAll` — i to nie jest
   * kwestia stylu. `beforeAll`, który rzuca, zamienia testy bloku w POMINIĘTE, a vitest
   * drukuje wtedy `Tests N skipped (N)`. Ten podpis stoi na liście `NOT_A_REAL_RED`
   * w harness/gate.py, więc bramka odrzuciłaby uczciwą czerwień jako "did not RUN".
   * To samo rzucenie wywołane z wnętrza testu jest zwykłą, policzalną porażką.
   */
  function tree(): string {
    if (dir === '') {
      const built = sandbox('density-exits');
      copyCheck(built, CHECK);
      // Sprawdzenie woła sędziego z scripts/, więc kopiujemy go razem z nim — dokładnie
      // tak, jak kryteria wołające cargo kopiują `checks/_cargo-serialize.sh`.
      copyScript(built, 'density-audit.mjs');
      plant(built, 'checks/density-baseline.json', BASELINE);
      // Sufit ma być PARSOWANY z dokumentu także przez skrypt, a kopia w piaskownicy jest
      // jedynym egzemplarzem, jaki kopia sprawdzenia widzi (ROOT liczy się z BASH_SOURCE).
      const doc = mustExist('docs/ARCHITECTURE.md', 'the only source of the seven density numbers');
      plant(built, 'docs/ARCHITECTURE.md', readFileSync(doc, 'utf8'));
      plant(built, 'src/main.tsx', 'export const nothing = 0;\n');
      dir = built;
    }
    return dir;
  }

  it('exits 0 for a snapshot that is measured and under the ratchet', () => {
    const run = runCheck(tree(), CHECK, [], withSnapshot('measured-zero.json'));

    expect(run.code, run.out).toBe(0);
    // NIEZMIENNIK 19: kod wyjścia to nie dowód. Zero bez powiedzenia, CO zmierzono i ILE
    // tego było, jest nie do odróżnienia od `exit 0` postawionego w pierwszej linii skryptu.
    expect(run.out, run.out).toMatch(/measured 7 of 7 metrics/);
    // Zmierzone zero jest wartością i ma być widoczne jako wartość, nie jako brak wpisu.
    expect(run.out).toMatch(/agentCardLines 0\/4/);
  });

  it('exits 1 for a snapshot with a metric above the ceiling', () => {
    const run = runCheck(tree(), CHECK, [], withSnapshot('one-over.json'));

    expect(run.code, run.out).toBe(1);
    // "Za gęsto" bez nazwy metryki i bez liczby nie daje się naprawić — to ta sama odmowa,
    // co "an architecture boundary was crossed" bez ścieżki pliku.
    expect(run.out).toMatch(/over the ceiling/i);
    expect(run.out).toMatch(/labelledRegions measured 9, ceiling 8/);
  });

  it('exits 1 when a metric is unmeasured and the collector gave no reason', () => {
    const run = runCheck(tree(), CHECK, [], withSnapshot('missing-metric.json'));

    expect(run.code, run.out).toBe(1);
    expect(run.out).toContain('agentCardLines');
    expect(run.out).toMatch(/no reason/i);
    // TA asercja odróżnia jedynkę od jedynki. Obie odmowy mają ten sam kod wyjścia, więc
    // bez niej skrypt mówiący zawsze "za gęsto" przechodzi oba przypadki — a człowiek szuka
    // wtedy regionu, którego nie ma za dużo, zamiast metryki, której nikt nie policzył.
    expect(run.out).not.toMatch(/over the ceiling/i);
  });

  it('exits 0 and prints the reason when the collector said what it could not measure', () => {
    const run = runCheck(tree(), CHECK, [], withSnapshot('unmeasured-with-reason.json'));

    expect(run.code, run.out).toBe(0);
    // Powód zapisany, nie zero. "Osie nawigacji" są osądem człowieka i to ma być powiedziane
    // przy każdym biegu, a nie ukryte pod wartością, która wygląda jak pomiar.
    expect(run.out).toMatch(/not measured:\s*navigationAxes\s*[—–-]\s*\S/);
    expect(run.out).toMatch(/human judgement/);
  });

  it('exits 2 when there is something to measure and it could not be measured', () => {
    // Bez zrzutu i bez dist/, ale src/main.tsx istnieje: front istnieje, pomiaru nie ma.
    // To NIE jest zieleń — to jest brak wyniku i ma się czytać inaczej niż "nie ma czego".
    const run = runCheck(tree(), CHECK);

    expect(run.code, run.out).toBe(2);
    expect(run.out).toMatch(/could not measure/i);
    // Komunikat musi nazwać rzecz, KTÓRA ISTNIEJE i której nie zmierzono. Bez tego "2"
    // czyta się jak awaria środowiska, a jest twierdzeniem o tym drzewie.
    expect(run.out).toContain('src/main.tsx');
    // I ta asercja rozstrzyga między dwoma wyjściami, których nie odróżnia sam komunikat:
    // "nie dało się" nie ma prawa użyć zdania, którym mówi się "nie ma czego".
    expect(run.out).not.toMatch(/nothing to measure/i);
  });

  it('exits 0 and names the missing path when there is nothing to measure at all', () => {
    // Drzewo bez src/main.tsx. Warunek pominięcia musi być MECHANICZNY i nazwany: pierwszy
    // plik pod src/ włącza sprawdzenie z powrotem, bez niczyjej decyzji.
    tree();
    const empty = sandbox('density-empty');
    copyCheck(empty, CHECK);
    copyScript(empty, 'density-audit.mjs');
    plant(empty, 'checks/density-baseline.json', BASELINE);
    const doc = mustExist('docs/ARCHITECTURE.md', 'the only source of the seven density numbers');
    plant(empty, 'docs/ARCHITECTURE.md', readFileSync(doc, 'utf8'));

    const run = runCheck(empty, CHECK);

    expect(run.code, run.out).toBe(0);
    expect(run.out).toContain('src/main.tsx');
    expect(run.out).toMatch(/nothing to measure/i);
    // Zieleń bez licznika jest czerwona (niezmiennik 19), także zieleń pominięcia: skrypt
    // ma powiedzieć, że policzył ZERO metryk, a nie milczeć i wyjść zerem.
    expect(run.out).toMatch(/0 metrics measured/);
  });
});
