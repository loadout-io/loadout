/* AC-5 dla T-22 — sędzia mówi, KTÓRA metryka i O ILE, a wartość równa limitowi przechodzi.
 *
 * Sędzia jest czystą funkcją nad zrzutem JSON i to jest cały powód, dla którego da się go
 * postawić pod kryterium: nie potrzebuje okna, przeglądarki ani `dist/`. Kolektor —
 * ta połowa, która naprawdę mierzy DOM — kryterium mieć nie może i to jest zapisane
 * w TASK.md pod "Świadomie poza zakresem": `NOT_A_REAL_RED` zawiera "Failed to launch"
 * i "Executable doesn't exist", więc kryterium wymagające Chromium daje w `before` czerwień,
 * którą bramka odrzuca, a w `full` zieleń, która nic nie znaczy. Repo źródłowe scertyfikowało
 * tak siedem kryteriów na przeglądarce, która nie startowała [03 §4.1].
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(judge(bad).verdict).toBe('over')`. Przechodzi ją
 * sędzia, który NIGDY nie mówi `pass` — a taki sędzia jest bezużyteczny dokładnie tak samo
 * jak taki, który nigdy nie mówi `over`. Rozstrzygają dwie rzeczy w tym samym pliku: zrzut
 * dokładnie na suficie musi dać `pass` (limit 8 znaczy "osiem wolno", nie "siedem"), a zrzut
 * mieszany musi wskazać DOKŁADNIE TE DWIE metryki po nazwie. "Za gęsto" bez nazwy metryki
 * i bez liczby nie daje się naprawić — to jest ta sama odmowa, co "an architecture boundary
 * was crossed" bez ścieżki pliku (AC-1).
 *
 * Zapadka jest tu pusta (`{}`). Metryka nieobecna w pliku zapadki jest przyjmowana przy
 * pierwszym pomiarze (AC-7), więc pusta zapadka nie wywiera nacisku i to kryterium mierzy
 * wyłącznie relację zrzut ↔ sufit.
 */
import { describe, expect, it } from 'vitest';
import { judge } from '../../scripts/density-audit.mjs';
import { CEILING_FIXTURE } from './_support';
import atCeiling from './fixtures/at-ceiling.json';
import mixed from './fixtures/mixed.json';
import oneOver from './fixtures/one-over.json';
import worseAtWide from './fixtures/worse-at-1512.json';

/** Zapadka bez ani jednego wpisu: pierwszy pomiar każdej metryki jest przyjmowany. */
const NO_RATCHET = {};

/** Limit tej metryki, z tej samej fikstury sufitu, którą dostaje sędzia. */
function limitOf(metric: string): number | undefined {
  return CEILING_FIXTURE.find((entry) => entry.key === metric)?.limit;
}

describe('judge weighs a snapshot against the ceiling', () => {
  it('passes a snapshot sitting exactly on the ceiling — eight allowed means eight', () => {
    const verdict = judge(atCeiling, CEILING_FIXTURE, NO_RATCHET);

    expect(verdict.verdict).toBe('pass');
    expect(verdict.over).toEqual([]);
  });

  it('names every metric that is over, with what it measured and what it may be', () => {
    const verdict = judge(oneOver, CEILING_FIXTURE, NO_RATCHET);

    expect(verdict.verdict).toBe('over');
    expect(verdict.over).toHaveLength(7);

    for (const entry of verdict.over) {
      const limit = limitOf(entry.metric);
      expect(limit, `over reports a metric no ceiling row declares: ${entry.metric}`).toBeDefined();
      expect(entry.limit).toBe(limit);
      // "O ile" jest połową wiadomości: sama nazwa nie mówi, czy to jeden region, czy dziesięć.
      expect(entry.measured).toBe((limit ?? 0) + 1);
    }
  });

  it('reports exactly the two metrics that are over, and not the five that are not', () => {
    const verdict = judge(mixed, CEILING_FIXTURE, NO_RATCHET);

    expect(verdict.verdict).toBe('over');
    expect(verdict.over.map((entry) => entry.metric).sort()).toEqual([
      'agentCardLines',
      'chromePixels',
    ]);
    expect(verdict.over.find((entry) => entry.metric === 'chromePixels')?.measured).toBe(120);
    expect(verdict.over.find((entry) => entry.metric === 'agentCardLines')?.measured).toBe(7);
  });

  it('takes the worse of the two window widths, so 1512 cannot hide behind 1100', () => {
    // 1100 px to najwęższe wspierane okno (DESIGN.md §9), 1512 px to szerokie [03 §4.1].
    // W tym zrzucie chrome mieści się przy 1100 (90) i nie mieści przy 1512 (120).
    // Sędzia mierzący jedną szerokość — albo biorący lepszą z dwóch — zameldowałby "pass".
    const verdict = judge(worseAtWide, CEILING_FIXTURE, NO_RATCHET);

    expect(verdict.verdict).toBe('over');
    expect(verdict.over).toHaveLength(1);
    expect(verdict.over[0]?.metric).toBe('chromePixels');
    expect(verdict.over[0]?.measured).toBe(120);
    expect(verdict.over[0]?.limit).toBe(96);
  });
});
