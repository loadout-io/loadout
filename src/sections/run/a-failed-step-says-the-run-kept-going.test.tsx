/* Nieudany krok mówi wprost, że bieg wykonał dla niego politykę „jedź dalej”.
 *
 * Fakt nie wynika z pracującego sąsiada ani ze strzałek. Równoległy korzeń może pracować
 * obok porażki, a potomek może ruszyć z innej przyczyny. Jedynym autorytetem jest wynik
 * schedulera `FailedAndCarriedOn`, wysłany osobną addytywną linią.
 *
 * SŁABA WERSJA testowałaby pole przekazane wprost do `RunTile`. Ten test zaczyna od kształtu
 * z drutu, przepuszcza go przez prawdziwe lustro i magazyn, a potem renderuje cały ekran Run.
 * Dowodzi więc zarówno przyjęcia faktu, jak i zdania widzianego przez człowieka
 * (niezmiennik 29).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { parseLine } from '../../ipc/types';
import type { FeedLine, Step } from '../../state/run';
import { useRun } from '../../state/run';
import { runFeed } from './feed/live';
import Run from './index';
import { closeAgent, openAgent } from './session/open';

const STEP_ID = 'build';
const STEP: Step = { id: STEP_ID, name: 'Build it', state: 'failed' };
const CARRIED_ON = {
  kind: 'stepCarriedOn',
  agent: STEP.name,
  stepId: STEP_ID,
};

function textOfCard(markup: string, stepId: string): string {
  const after = markup.split(`data-step="${stepId}"`)[1] ?? '';
  const card = after.split('</button>')[0] ?? '';
  return card
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function card(): string {
  return textOfCard(renderToStaticMarkup(<Run />), STEP_ID);
}

function openAgentStatus(markup: string, agent: string): string {
  const screen = markup.split(`data-agent-screen="${agent}"`)[1] ?? '';
  return screen.match(/data-status(?:="")?[^>]*>([^<]*)<\/span>/)?.[1] ?? '';
}

beforeEach(() => {
  closeAgent();
  useRun.setState({
    workflow: 'Carry on explicitly',
    steps: [STEP],
    links: [],
    lines: [],
    agents: [],
  });
});

describe('a failed step says the scheduler carried on', () => {
  it('takes the explicit wire fact through the store to the real tile', () => {
    const parsed = parseLine(CARRIED_ON);
    expect(
      parsed,
      'the exact additive Rust wire shape was rejected before the store could see it',
    ).not.toBeNull();
    if (parsed === null) return;

    useRun.getState().appendLines([{ ...parsed, id: 1, at: 1_000 }]);

    expect(
      useRun.getState().steps[0],
      'the store accepted the line but did not attach its fact to the matching workflow step',
    ).toMatchObject({ carriedOn: true });
    expect(
      card(),
      'the real run tile stayed ambiguous after the scheduler explicitly said it carried on',
    ).toContain('failed — carried on');
  });

  it('makes one carry-on wire fact the whole terminal outcome', () => {
    useRun.setState({ steps: [{ ...STEP, state: 'running' }] });
    const parsed = parseLine(CARRIED_ON);
    expect(parsed).not.toBeNull();
    if (parsed === null) return;

    useRun.getState().appendLines([{ ...parsed, id: 1, at: 1_000 }]);

    expect(
      useRun.getState().steps[0],
      'a lossy queue can deliver one line, so the carry-on fact itself must say both failed and continued',
    ).toMatchObject({ state: 'failed', carriedOn: true });
    expect(card()).toContain('failed — carried on');
  });

  it('does not invent carry-on without that explicit fact', () => {
    expect(card()).toContain('failed');
    expect(card()).not.toContain('carried on');
  });

  it('clears the old fact when the reused tile enters a new state', () => {
    const carried = parseLine(CARRIED_ON);
    const running = parseLine({
      kind: 'stepState',
      agent: STEP.name,
      stepId: STEP_ID,
      state: 'running',
    });
    expect(carried).not.toBeNull();
    expect(running).not.toBeNull();
    if (carried === null || running === null) return;

    useRun.getState().appendLines([
      { ...carried, id: 1, at: 1_000 },
      { ...running, id: 2, at: 2_000 },
    ]);

    expect(useRun.getState().steps[0]).toMatchObject({ state: 'running', carriedOn: false });
    expect(card()).not.toContain('carried on');
  });

  it('ignores a carry-on fact for a step outside the plan without churning the plan', () => {
    const foreign = parseLine({ ...CARRIED_ON, stepId: 'not-in-this-run' });
    expect(foreign).not.toBeNull();
    if (foreign === null) return;
    const before = useRun.getState().steps;

    useRun.getState().appendLines([{ ...foreign, id: 1, at: 1_000 }]);

    expect(useRun.getState().steps).toBe(before);
    expect(card()).not.toContain('carried on');
  });

  it('lets the terminal scheduler outcome beat an earlier successful agent turn', () => {
    useRun.setState({ steps: [{ ...STEP, state: 'running' }] });
    const done: FeedLine = {
      kind: 'done',
      agent: STEP.name,
      text: 'Done · 1 turn · 1.0s',
      turns: 1,
      durationMs: 1_000,
      costUsd: null,
      inputTokens: 0,
      outputTokens: 0,
      cachedTokens: 0,
      ended: 'well',
      id: 1,
      at: 1_000,
    };
    const carried = parseLine(CARRIED_ON);
    expect(carried).not.toBeNull();
    if (carried === null) return;

    runFeed.appendLines([done, { ...carried, id: 2, at: 2_000 }]);
    useRun.getState().appendLines([{ ...carried, id: 2, at: 2_000 }]);

    expect(
      card(),
      'Done describes the vendor turn; the later scheduler outcome describes the workflow step',
    ).toContain('failed — carried on');

    openAgent(STEP.name, STEP_ID);
    expect(
      openAgentStatus(renderToStaticMarkup(<Run />), STEP.name),
      'opening the same step showed the earlier vendor turn instead of the terminal scheduler outcome',
    ).toBe('failed');
  });

  it('keeps the outcomes of two legal same-named steps separate', () => {
    const other: Step = { id: 'verify', name: STEP.name, state: 'running' };
    useRun.setState({ steps: [{ ...STEP, carriedOn: true }, other] });
    const done: FeedLine = {
      kind: 'done',
      agent: STEP.name,
      text: 'Done · 1 turn · 1.0s',
      turns: 1,
      durationMs: 1_000,
      costUsd: null,
      inputTokens: 0,
      outputTokens: 0,
      cachedTokens: 0,
      ended: 'well',
      id: 91,
      at: 91_000,
    };
    runFeed.appendLines([done]);

    const markup = renderToStaticMarkup(<Run />);
    expect(
      textOfCard(markup, STEP_ID),
      'the status of the later same-named step replaced the failed step by name',
    ).toContain('failed — carried on');
    expect(
      textOfCard(markup, other.id),
      'a shared Done line was arbitrarily assigned to the other same-named running step',
    ).toContain('working');

    openAgent(STEP.name, STEP_ID);
    expect(
      openAgentStatus(renderToStaticMarkup(<Run />), STEP.name),
      'the detail screen forgot which of the same-named steps the person opened',
    ).toBe('failed');

    openAgent(STEP.name, other.id);
    expect(
      openAgentStatus(renderToStaticMarkup(<Run />), STEP.name),
      'the detail screen assigned a shared Done line to the concrete running step',
    ).toBe('working');

    openAgent(STEP.name);
    expect(
      renderToStaticMarkup(<Run />),
      'opening an ambiguous same-named agent from the palette offered to rerun an arbitrary step',
    ).not.toContain('data-run-again=');
  });
});
