/* SKOŃCZONY BIEG ZOSTAJE JEDNYM ZDANIEM NA CAŁYM EKRANIE — nagłówek i kafelki czytają ten
 * sam stan. Kryterium montuje produkcyjny `<Run />`, bo sprawdzenie samego modelu nie dowodzi,
 * że zdanie albo stany dochodzą tam, gdzie widzi je człowiek (niezmiennik 29).
 *
 * 2026-09 (Z-37) — TEST POWSTAŁ PRZED POPRAWKĄ. Ze szkieletem `runEnded()` zachowującym
 * dotychczasowe czyszczenie wykonał się i padł na pierwszej asercji: ekran powiedział
 * `Ready to run` nad trzema kafelkami `waiting`. To jest dokładnie stan ze zrzutu właściciela,
 * nie błąd zbierania pliku (AGENTS.md §2a punkt 4).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';
import type { FeedLine, Step } from '../../state/run';

const { invoked, finishRun } = vi.hoisted(() => {
  let finish: (() => void) | null = null;
  return {
    invoked: vi.fn((command: string) => {
      if (command !== 'run_workflow') return Promise.resolve(undefined);
      return new Promise<void>((resolve) => {
        finish = resolve;
      });
    }),
    finishRun: (): void => {
      finish?.();
      finish = null;
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('./index')).default;
const { start } = await import('./io');
const { runFor } = await import('../../state/run');
const { headlineFor } = await import('./strip/headline');
const { useWorkspaces } = await import('../../state/workspaces');
const { forgetWhatIsReady, rememberAgents, rememberRuns, rememberWorkflows } =
  await import('./whats-ready');

const HERE = { id: '/Users/x/finished-run', name: 'Finished run', folder: '/Users/x/finished-run' };
const NEXT_DOOR = { id: '/Users/x/other-run', name: 'Other run', folder: '/Users/x/other-run' };
const STARTED = Date.UTC(2026, 8, 4, 14, 21);
const ENDED = STARTED + 115 * 60_000;

const STEPS: readonly Step[] = [
  { id: 'build', name: 'Build', state: 'pending', kind: 'agent' },
  { id: 'check', name: 'Check', state: 'pending', kind: 'check' },
  { id: 'report', name: 'Report', state: 'pending', kind: 'agent' },
];

const WORKFLOW: Choice = {
  path: 'qa.json',
  name: 'QA',
  steps: STEPS,
  links: [],
};

function stateLine(id: number, stepId: string, state: string): FeedLine {
  return { kind: 'stepState', agent: stepId, stepId, state, id, at: STARTED + id - 1 };
}

const DONE: FeedLine = {
  kind: 'done',
  agent: 'Report',
  text: 'Done',
  turns: 103,
  durationMs: 11 * 60_000 + 22_000,
  costUsd: 5.88,
  inputTokens: 0,
  outputTokens: 0,
  cachedTokens: 0,
  ended: 'well',
  id: 4,
  at: ENDED,
};

function finishInTheStore(): ReturnType<typeof runFor> {
  const session = runFor(HERE.folder);
  session.getState().nowRunning('QA', STEPS, HERE.folder, WORKFLOW.path, []);
  session
    .getState()
    .appendLines([
      stateLine(1, 'build', 'succeeded'),
      stateLine(2, 'check', 'failed'),
      stateLine(3, 'report', 'cancelled'),
      DONE,
    ]);
  session.getState().runEnded();
  return session;
}

function tile(markup: string, id: string): string {
  const at = markup.indexOf(`data-step="${id}"`);
  if (at < 0) return '';
  const next = markup.indexOf('data-step="', at + 1);
  return markup.slice(at, next < 0 ? markup.length : next);
}

beforeEach(() => {
  invoked.mockClear();
  useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
  runFor(HERE.folder).setState({
    lines: [],
    droppedBefore: 0,
    earliestKnownId: null,
    agents: [],
    steps: [],
    workflow: '',
    fileName: '',
    folder: null,
    links: null,
    ended: null,
    answers: [],
  });
  rememberWorkflows([WORKFLOW]);
  rememberAgents(1);
  rememberRuns(HERE.folder, []);
});

afterEach(() => {
  forgetWhatIsReady();
});

describe('a finished run keeps the outcome a person just watched', () => {
  it('says Finished over the states the run really ended on, not Ready to run over waiting tiles', () => {
    finishInTheStore();
    /* Przejście do sąsiedniej sesji i powrót przed montażem: sam odczyt `runFor(HERE)` nie
     * dowodzi, że uchwyt produkcyjnego ekranu wraca do zachowanego magazynu. */
    useWorkspaces.setState({ all: [HERE, NEXT_DOOR], activeId: NEXT_DOOR.id, said: null });
    useWorkspaces.setState({ all: [HERE, NEXT_DOOR], activeId: HERE.id, said: null });
    const markup = renderToStaticMarkup(<Run />);

    expect(
      markup,
      'the stream has ended but the run head still offers the workflow as if none of its work ' +
        'had happened',
    ).toMatch(/Finished · started \d\d:\d\d · 1 h 55 min/u);
    expect(tile(markup, 'build')).toContain('done');
    expect(tile(markup, 'check')).toContain('failed');
    expect(tile(markup, 'report')).toContain('stopped');
    expect(tile(markup, 'build') + tile(markup, 'check') + tile(markup, 'report')).not.toContain(
      'waiting',
    );
  });

  it('the headline model reads the same finished flag and terminal plan as the screen', () => {
    const state = finishInTheStore().getState();
    const headline = headlineFor({
      workflow: state.workflow,
      finished: state.ended?.name ?? '',
      startedAt: state.ended?.startedAt ?? null,
      nextUp: WORKFLOW.name,
      steps: state.steps,
      lines: state.lines,
      droppedBefore: state.droppedBefore,
      workspace: HERE.name,
      agents: state.agents.length,
      budgetUsd: null,
    });

    expect(headline.tone).toBe('ended');
    expect(headline.title).toBe('QA');
    expect(headline.eyebrow).toMatch(/^Finished · started \d\d:\d\d · 1 h 55 min$/u);
  });

  it('marks the run as finished through the same finally edge that releases Start', async () => {
    const running = start(WORKFLOW.path, 3, WORKFLOW, HERE.folder);
    expect(runFor(HERE.folder).getState().workflow).toBe('QA');

    finishRun();
    await running;

    const state = runFor(HERE.folder).getState();
    expect(state.workflow, 'the finished run must no longer be stoppable').toBe('');
    expect(
      state.ended?.name,
      'the command returned through its production finally edge and the store forgot which run ended',
    ).toBe('QA');
    expect(state.steps.map((step) => step.id)).toEqual(STEPS.map((step) => step.id));
  });
});
