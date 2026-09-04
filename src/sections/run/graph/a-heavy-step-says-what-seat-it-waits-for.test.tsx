/* Karta gotowego ciężkiego kroku mówi człowiekowi, na jakie miejsce czeka.
 *
 * Plan idzie produkcyjną drogą `freshStep` → `planOf` → magazyn biegu, a stan `ready`
 * przyjeżdża wierszem `stepState`. Ręcznie złożony `GraphStep` ominąłby obie krawędzie i byłby
 * zielonym testem nad martwą funkcją (niezmiennik 29).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import type { FeedLine } from '../../../state/run';
import { useRun } from '../../../state/run';
import type { Step as FileStep } from '../../../state/workflows';
import { freshStep } from '../../workflows/canvas/connect';
import { planOf } from '../choices';
import Run from '../index';

const HEAVY = 'heavy';
const ORDINARY = 'ordinary';
const WAITING = 'waiting for the heavy seat';
const AT = { x: 24, y: 24 };

function fileSteps(): readonly FileStep[] {
  const heavy = freshStep('agent', HEAVY, AT);
  const ordinary = freshStep('agent', ORDINARY, { x: 24, y: 168 });
  if (heavy.kind !== 'agent' || ordinary.kind !== 'agent') {
    throw new Error('freshStep did not build the two requested agent steps');
  }
  return [
    { ...heavy, name: 'Build everything', weight: 'heavy' },
    { ...ordinary, name: 'Write notes' },
  ];
}

function ready(id: number, stepId: string, agent: string): FeedLine {
  return { kind: 'stepState', id, at: id * 1_000, stepId, agent, state: 'ready' };
}

function textOfCard(markup: string, id: string): string {
  const starts = markup.indexOf(`data-step="${id}"`);
  if (starts < 0) return '';
  const rest = markup.slice(starts);
  const next = rest.indexOf('data-step="', 1);
  return (next < 0 ? rest : rest.slice(0, next))
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

beforeEach(() => {
  const steps = planOf(fileSteps());
  useRun.setState({ workflow: 'Heavy work', steps, links: [], lines: [], agents: [] });
  useRun
    .getState()
    .appendLines([ready(1, HEAVY, 'Build everything'), ready(2, ORDINARY, 'Write notes')]);
});

describe('a heavy step waiting in the run names the scarce seat', () => {
  it('puts the sentence on the heavy card and keeps the ordinary card unchanged', () => {
    const markup = renderToStaticMarkup(<Run />);
    const heavy = textOfCard(markup, HEAVY);
    const ordinary = textOfCard(markup, ORDINARY);

    expect(heavy, 'the production run screen did not mount the heavy step card').not.toBe('');
    expect(
      heavy,
      'the heavy ready step still describes an arrow instead of its current wait',
    ).toContain(WAITING);
    expect(
      ordinary,
      'the ordinary ready step lost the existing answer about its place in the graph',
    ).toContain('first step');
    expect(
      ordinary,
      'the sentence leaked onto a step that does not take the heavy seat',
    ).not.toContain(WAITING);
  });
});
