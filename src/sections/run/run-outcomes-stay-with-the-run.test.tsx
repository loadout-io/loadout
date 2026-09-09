import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const wire = vi.hoisted(() => ({
  runs: [] as Array<{ send: (batch: unknown[]) => void; finish: () => void }>,
}));
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: ((batch: unknown[]) => void) | null = null;
  },
  invoke: (command: string, args: { lines: { onmessage: (batch: unknown[]) => void } }) => {
    if (command !== 'run_workflow') return Promise.resolve(undefined);
    return new Promise<void>((resolve) => {
      wire.runs.push({ send: (batch) => args.lines.onmessage(batch), finish: resolve });
    });
  },
}));
const Run = (await import('./index')).default;
const { start } = await import('./io');
const { runFor } = await import('../../state/run');
const { useWorkspaces } = await import('../../state/workspaces');
const { feedFor } = await import('./feed/live');
const { rememberWorkflows, rememberAgents, rememberRuns } = await import('./whats-ready');
const { planOfPastRun } = await import('./history-command');
import type { PastRun } from './io';

const HERE = '/tmp/loadout-run-outcomes';
const BEGIN = Date.UTC(2026, 8, 9, 21, 26);
const steps = [
  { id: 'plan', name: 'Plan implementation', kind: 'agent' as const, state: 'pending' as const },
  { id: 'qa', name: 'QA', kind: 'agent' as const, state: 'pending' as const },
];
const workflow = { path: 'test.json', name: 'Murmur', steps, links: [{ from: 'plan', to: 'qa' }] };
function progress(status: string) {
  return {
    kind: 'runProgress',
    agent: 'Loadout',
    runId: 'run-new',
    name: 'Murmur',
    status,
    startedAt: BEGIN,
    endedAt: status === 'running' ? null : BEGIN + 20 * 60_000,
    steps: [
      {
        id: 'plan',
        tileId: 'plan',
        name: 'Plan implementation',
        kind: 'agent',
        state: status === 'running' ? 'running' : 'failed',
        carriedOn: false,
        processStarted: true,
        error: status === 'running' ? '' : 'No plan was published.',
        dependsOn: [],
      },
      {
        id: 'qa',
        tileId: 'qa',
        name: 'QA',
        kind: 'agent',
        state: status === 'running' ? 'pending' : 'skipped',
        carriedOn: false,
        processStarted: false,
        error: status === 'running' ? '' : 'Skipped: Plan implementation did not provide a plan.',
        dependsOn: ['plan'],
      },
    ],
  };
}
function tile(html: string, id: string) {
  const start = html.indexOf(`data-step="${id}"`);
  let depth = 1;
  const tags = /<div\b|<\/div>/gu;
  tags.lastIndex = start;
  for (let tag = tags.exec(html); tag !== null; tag = tags.exec(html)) {
    depth += tag[0] === '</div>' ? -1 : 1;
    if (depth === 0) return html.slice(start, tags.lastIndex);
  }
  throw new Error('The step card has no closing element');
}
beforeEach(() => {
  wire.runs.length = 0;
  useWorkspaces.setState({
    all: [{ id: HERE, name: 'Test', folder: HERE }],
    activeId: HERE,
    said: null,
  });
  runFor(HERE).setState({
    workflow: '',
    steps: [],
    ended: null,
    progress: null,
    lines: [],
    agents: [],
    droppedBefore: 0,
  });
  feedFor(HERE).runEnded();
  rememberWorkflows([workflow]);
  rememberAgents(1);
  rememberRuns(HERE, []);
});

describe('the current run owns its result and cards', () => {
  it('uses unique identities for repeated attempts restored from history', () => {
    const past = {
      steps: [
        { id: 'attempt-1', tile: 'qa', name: 'QA', state: 'failed' },
        { id: 'attempt-2', tile: 'qa', name: 'QA', state: 'succeeded' },
        { id: 'attempt-3', tile: 'qa', name: 'QA', state: 'not_run' },
      ],
    } as unknown as PastRun;
    const plan = planOfPastRun(past);
    expect(new Set(plan.map((step) => step.id)).size).toBe(3);
    expect(plan.map((step) => step.state)).toEqual(['failed', 'succeeded', 'skipped']);
  });

  it('shows the backend failure, its cause and elapsed time instead of green Finished', async () => {
    const run = start(workflow.path, 3, workflow, HERE);
    const port = wire.runs[0];
    expect(port).toBeDefined();
    port?.send([progress('failed')]);
    port?.finish();
    await run;
    const html = renderToStaticMarkup(<Run />);
    expect(html).toMatch(/Failed · started \d\d:\d\d · 20 min/u);
    expect(html).not.toContain('Finished ·');
    expect(tile(html, 'plan')).toContain('No plan was published.');
    expect(tile(html, 'qa')).toContain('not run');
    expect(html).toContain('1 agent');
  });

  it('keeps a new pending QA independent of the previous run with the same names', async () => {
    const previous = start(workflow.path, 3, workflow, HERE);
    const old = wire.runs[0];
    old?.send([
      {
        kind: 'done',
        agent: 'QA',
        text: 'Done',
        vendorTurns: 1,
        durationMs: 200,
        costUsd: 1,
        uncachedInput: 0,
        cacheRead: 0,
        cacheWrite: 0,
        output: 0,
        ended: 'badly',
      },
    ]);
    old?.finish();
    await previous;
    const current = start(workflow.path, 3, workflow, HERE);
    try {
      wire.runs[1]?.send([progress('running')]);
      old?.send([{ kind: 'stepState', agent: 'QA', stepId: 'qa', state: 'failed' }]);
      const html = renderToStaticMarkup(<Run />);
      expect(tile(html, 'qa')).toContain('waiting');
      expect(tile(html, 'qa')).not.toContain('failed');
      expect(tile(html, 'qa')).not.toContain('Done');
    } finally {
      wire.runs[1]?.finish();
      await current;
    }
  });

  it('does not restore the previous failure after leaving its result for a workflow preview', async () => {
    const running = start(workflow.path, 3, workflow, HERE);
    wire.runs[0]?.send([progress('failed')]);
    wire.runs[0]?.finish();
    await running;
    runFor(HERE).getState().forgetTheLastRun();
    runFor(HERE)
      .getState()
      .appendLines([
        {
          kind: 'note',
          agent: 'Lead',
          text: 'Ready for your next task.',
          body: [],
          id: 10_000,
          at: BEGIN,
        },
      ]);
    const html = renderToStaticMarkup(<Run />);
    expect(tile(html, 'plan')).toContain('waiting');
    expect(tile(html, 'plan')).not.toContain('No plan was published.');
  });
});
