/* Kryterium 5: pasek loadoutu mówi prawdę także wtedy, gdy biegnie kilka kroków naraz.
 *
 * `expect(blocks[2].state).toBe('now')` przechodzi dla implementacji z jednym kursorem
 * `currentIndex` — a taka implementacja jest poprawna dokładnie do pierwszego biegu, w którym
 * dwa kroki idą równolegle. Równoległość jest całą przesłanką tego produktu (niezmiennik 11):
 * poprzedni prototyp miał `max_parallel`, które było tylko szerokością wysyłki, i cztery „równoległe"
 * pasy lądowały w rozłącznych oknach po pół sekundy.
 *
 * Trzy rzeczy rozróżniają dobry pasek od ładnego:
 *   - dwa kroki `running` dają DWA bloki `now` i podpis w liczbie mnogiej,
 *   - żaden z trzech stanów końcowych bez sukcesu (`failed`, `cancelled`, `skipped`) nie daje
 *     `done`; blok wypełniony jest obietnicą, że krok się udał,
 *   - workflow o trzech krokach daje trzy bloki, nigdy cztery „jak w makiecie" (niezmiennik 17).
 */
import { describe, expect, it } from 'vitest';
import type { Step, StepState } from '../../../state/run';
import { stripFor } from './model';

const WORKFLOW = 'Fix the CSV parser';

function step(name: string, state: StepState): Step {
  return { id: name.toLowerCase(), name, state };
}

/** Siedem stanów kroku [ARCHITECTURE §5] i to, czym każdy jest na pasku [DESIGN §2]. */
const MAPPING: ReadonlyArray<readonly [StepState, 'done' | 'now' | 'todo', boolean]> = [
  ['succeeded', 'done', false],
  ['running', 'now', false],
  ['pending', 'todo', false],
  ['ready', 'todo', false],
  ['failed', 'todo', true],
  ['cancelled', 'todo', true],
  ['skipped', 'todo', true],
];

describe('the loadout strip tells the truth with several steps running at once', () => {
  it('maps all seven step states, and none of the three dead ends reads as done', () => {
    const steps = MAPPING.map(([state]) => step(state, state));
    const { blocks } = stripFor(WORKFLOW, steps);

    expect(blocks.length, 'one block per step, seven steps, seven blocks').toBe(MAPPING.length);
    MAPPING.forEach(([state, want, ended], i) => {
      const block = blocks[i];
      expect(block?.state, 'a step that is ' + state + ' draws as ' + want).toBe(want);
      expect(
        block?.ended,
        'and it says separately whether it is over: a run stopped halfway has to look ' +
          'different from one still waiting its turn',
      ).toBe(ended);
    });

    for (const state of ['failed', 'cancelled', 'skipped'] as const) {
      const only = stripFor(WORKFLOW, [step('Build', state)]).blocks;
      expect(
        only.map((block) => block.state),
        'a step that is ' +
          state +
          ' may never draw as done. A filled block promises the step worked, and a skipped ' +
          'step shown as finished is a lie the user only finds out about at the end',
      ).toEqual(['todo']);
    }
  });

  it('draws one block per step, in graph order, and never a fourth one from the mockup', () => {
    const { blocks } = stripFor(WORKFLOW, [
      step('Plan', 'succeeded'),
      step('Build', 'running'),
      step('Check', 'pending'),
    ]);

    expect(
      blocks.map((block) => block.name),
      'the strip has exactly as many blocks as the run has steps, in the order of the ' +
        'workflow. Four blocks because the mockup has four is the interface drawing a ' +
        'relationship that is not in the data (invariant 17)',
    ).toEqual(['Plan', 'Build', 'Check']);
  });

  it('says "step N of M" while exactly one step is running', () => {
    const strip = stripFor(WORKFLOW, [
      step('Plan', 'succeeded'),
      step('Build', 'running'),
      step('Check', 'pending'),
      step('Ship', 'pending'),
    ]);

    expect(strip.blocks.filter((block) => block.state === 'now').length).toBe(1);
    expect(strip.caption, 'one step running, so the caption can name it by number').toBe(
      WORKFLOW + ' · step 2 of 4',
    );
  });

  it('counts them in the plural once two steps run at the same time', () => {
    const strip = stripFor(WORKFLOW, [
      step('Plan', 'succeeded'),
      step('Build', 'running'),
      step('Check', 'running'),
      step('Ship', 'pending'),
    ]);

    expect(
      strip.blocks.filter((block) => block.state === 'now').length,
      'two steps running means TWO blocks in the accent colour. One cursor through the list ' +
        'passes every sequential run and lies the first time two agents work at once',
    ).toBe(2);
    expect(
      strip.caption,
      'and the caption stops pretending there is a single place the run is at',
    ).toBe(WORKFLOW + ' · 2 of 4 running');
  });

  it('falls back to the plain count when nothing is running', () => {
    const strip = stripFor(WORKFLOW, [
      step('Plan', 'succeeded'),
      step('Build', 'failed'),
      step('Check', 'skipped'),
      step('Ship', 'cancelled'),
    ]);

    expect(strip.blocks.filter((block) => block.state === 'now').length).toBe(0);
    expect(strip.caption, 'nothing is running, so there is no step number to point at').toBe(
      WORKFLOW + ' · 4 steps',
    );
    expect(
      strip.blocks.filter((block) => block.state === 'done').length,
      'and only the step that actually succeeded is filled in',
    ).toBe(1);
  });
});
