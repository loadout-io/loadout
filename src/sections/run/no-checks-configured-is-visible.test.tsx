/* AC-1: bieg, w którego planie nie ma ani jednego kafelka „sprawdź", mówi to CZŁOWIEKOWI.
 *
 * Decyzja D7, akapit „Co musi przetrwać nawet przy zerowej ceremonii": przy workflow bez
 * sprawdzeń UI mówi „no checks configured", uczciwie, i nie pokazuje zieleni. Brak ceremonii
 * ma znaczyć „nikt tego nie sprawdził", nigdy „sprawdzone i dobrze".
 *
 * SŁABĄ WERSJĄ tego kryterium jest wywołanie `stripFor(...)` albo nowego predykatu wprost
 * i sprawdzenie zwróconej wartości. Taka wersja przechodzi nad dokładnie tą wadą, dla której
 * to repo powstało: funkcja umie złożyć zdanie, a do człowieka ono nie dociera, bo rodzaj kroku
 * gubi się po drodze do magazynu biegu (niezmiennik 29 — kryterium zielone, funkcja martwa).
 * Dlatego niżej montuje się CAŁY produkcyjny ekran Run i czyta jego markup.
 *
 * PLAN POWSTAJE PRODUKCYJNĄ DROGĄ: `freshStep` (ta sama fabryka, którą płótno stawia kafelki)
 * → `planOf` → magazyn biegu. Krok z ręcznie dopisanym rodzajem obchodziłby jedyną krawędź,
 * która dziś tego rodzaju nie przewozi — i zazieleniłby kryterium nad drogą, której nie ma.
 *
 * DWIE POŁOWY, bo jedna nie wystarcza. Sam napis dowodzi tylko, że zdanie umie się pojawić:
 * implementacja pisząca je ZAWSZE przeszłaby tę połowę i kłamałaby o każdym biegu, który
 * sprawdzenie ma. Druga połowa odróżnia „policzone z planu" od „wpisane na stałe", a asercja
 * o podpisie paska pilnuje, żeby ta druga nie przeszła na markupie, który się nie zamontował.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { useRun } from '../../state/run';
import type { Step as FileStep } from '../../state/workflows';
import { freshStep } from '../workflows/canvas/connect';
import { planOf } from './choices';
import Run from './index';

/** Zdanie z D7, co do znaku. */
const NO_CHECKS = 'no checks configured';

/** Jak workflow nazywa sam siebie — to on stoi w podpisie paska loadoutu. */
const WORKFLOW = 'Ship it';

const AT = { x: 24, y: 24 };

/** Plan, w którym nikt niczego nie sprawdza: agent i punkt kontrolny, czyli ceremonia bez
 * ani jednego pomiaru. Punkt kontrolny stoi tu z premedytacją — zdanie ma się brać z rodzaju
 * `check`, a nie z tego, że wszystkie kafelki są jednakowe. */
const WITHOUT_A_CHECK: readonly FileStep[] = [
  freshStep('agent', 'write', AT),
  freshStep('checkpoint', 'look-at-it', AT),
];

/** Ten sam plan z kafelkiem „sprawdź" na drugim miejscu — i to jest ich jedyna różnica. */
const WITH_A_CHECK: readonly FileStep[] = [
  freshStep('agent', 'write', AT),
  freshStep('check', 'run-the-checks', AT),
];

/** Markup produkcyjnego ekranu Run dla biegu o tym planie. */
function screenFor(steps: readonly FileStep[]): string {
  useRun.setState({ workflow: WORKFLOW, steps: planOf(steps), lines: [] });
  return renderToStaticMarkup(<Run />);
}

describe('a run whose plan has no check tile admits it on screen', () => {
  it('carries the sentence a person can read', () => {
    expect(
      screenFor(WITHOUT_A_CHECK),
      'the mounted run screen says nothing about checks. An agent tile and a checkpoint tile ' +
        'measure nothing, and D7 makes the screen say so: silence has to read as nobody ' +
        'looked, never as looked and it was fine.',
    ).toContain(NO_CHECKS);
  });

  it('says nothing of the sort once a check tile stands in the plan', () => {
    const markup = screenFor(WITH_A_CHECK);

    expect(
      markup,
      'the run screen carries no caption for this run at all, so the half below would pass on ' +
        'markup that never mounted the loadout bar.',
    ).toContain(WORKFLOW);
    expect(
      markup,
      'the same screen, with a check tile standing in the plan, still carried the sentence. ' +
        'Writing it always is not reading the plan: it would tell a person that a run which ' +
        'does check its work checks nothing.',
    ).not.toContain(NO_CHECKS);
  });
});
