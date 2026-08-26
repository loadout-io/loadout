/* T-131 AC-3: the current Agent screen excludes every note the current prompt excludes.
 *
 * Only the Tauri transport is replaced. The test calls production readWhatWasGiven, opens a
 * real agent tile and renders AgentScreen. A second render proves that leftOut remains in the
 * real Memory catalog instead of being deleted to make the Agent screen look correct.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Note } from '../../../state/memory';
import type { FeedLine, Step } from '../../../state/run';

const BUILD = 'Build';
const CHECK = 'Check';
const FOLDER = '/Users/x/t131-project';

type AcceptanceNote = Note & { readonly leftOut?: boolean };

function note(id: string, rule: string, fields: Partial<AcceptanceNote>): AcceptanceNote {
  const current: Note = {
    place: 'library',
    id,
    title: id,
    rule,
    because: 'T131 fixture reason',
    status: 'in-use',
    scope: 'everywhere',
    length: 31,
    occurrences: 1,
    modified: '2026-08-26T10:00:00Z',
  };
  return { ...current, ...fields };
}

const EVERYWHERE = note('given-everywhere', 'T131 global note reached Build', {
  leftOut: false,
});
const MINE = note('given-to-build', 'T131 Build-only note reached Build', {
  scope: 'this-agent',
  agent: BUILD,
  leftOut: false,
});
const PROJECT = note('given-to-project', 'T131 project note reached Build', {
  place: 'project',
  scope: 'this-project',
  leftOut: false,
});
const CANDIDATE = note('not-approved', 'T131 suggested note stayed outside prompts', {
  status: 'suggested',
  leftOut: false,
});
const ANOTHER_AGENTS = note('given-to-check', 'T131 Check-only note stayed away from Build', {
  scope: 'this-agent',
  agent: CHECK,
  leftOut: false,
});
const LEFT_OUT = note('left-out', 'T131 length-limited note stayed outside prompts', {
  leftOut: true,
});
const LIBRARY_PROJECT_SCOPE = note(
  'library-project-scope',
  'T131 misplaced library project note stayed outside prompts',
  {
    scope: 'this-project',
    leftOut: false,
  },
);
const PROJECT_GLOBAL = note(
  'project-global',
  'T131 misplaced project global note stayed outside prompts',
  {
    place: 'project',
    scope: 'everywhere',
    leftOut: false,
  },
);

const NOTES: readonly AcceptanceNote[] = [
  EVERYWHERE,
  MINE,
  PROJECT,
  CANDIDATE,
  ANOTHER_AGENTS,
  LEFT_OUT,
  LIBRARY_PROJECT_SCOPE,
  PROJECT_GLOBAL,
];

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((command: string, _args?: unknown): Promise<unknown> => {
    if (command === 'list_notes') return Promise.resolve([]);
    return Promise.reject(new Error(`T131 does not mock ${command}`));
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { AgentScreen, readWhatWasGiven } = await import('./mount');
const { closeAgent, openAgent } = await import('./open');
const { roster } = await import('../rail/roster');
const { runFeed } = await import('../feed/live');
const { useRun } = await import('../../../state/run');
const { useMemory } = await import('../../../state/memory');
const { default: MemoryScreen } = await import('../../memory');
const { line } = await import('../feed/fixtures/lines');

const STEPS: readonly Step[] = [
  { id: 'build', name: BUILD, state: 'running' },
  { id: 'check', name: CHECK, state: 'pending' },
];
const LINES: readonly FeedLine[] = [
  line.note(1, 0, BUILD, 'Implementing the current memory boundary.'),
];

useRun.setState({ steps: STEPS, lines: [...LINES], folder: FOLDER });
runFeed.appendLines(LINES);
const cards = roster({
  view: runFeed.view,
  agents: STEPS.map((step) => ({
    id: step.name,
    name: step.name,
    role: '',
    step: step.state,
    stepId: step.id,
  })),
});

invoked.mockImplementation((command: string, _args?: unknown): Promise<unknown> => {
  if (command === 'list_notes') return Promise.resolve(NOTES);
  return Promise.reject(new Error(`T131 does not mock ${command}`));
});
await readWhatWasGiven(FOLDER);
openAgent(BUILD);

function visible(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function agentWords(): string {
  const markup = renderToStaticMarkup(<AgentScreen cards={cards} />);
  const start = markup.indexOf('data-agent-screen');
  return visible(start < 0 ? '' : markup.slice(start));
}

function memoryRow(one: AcceptanceNote): string {
  useMemory.setState({
    notes: [...NOTES],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
  });
  const markup = renderToStaticMarkup(<MemoryScreen store={useMemory} />);
  const marker = `data-note-address="${one.place}:${one.id}"`;
  const start = markup.indexOf(marker);
  if (start < 0) return '';
  const end = markup.indexOf('</li>', start);
  return visible(end < 0 ? markup.slice(start) : markup.slice(start, end));
}

const build = agentWords();
const leftOutInCatalog = memoryRow(LEFT_OUT);
closeAgent();

describe('the current Agent screen shows only notes the current prompt received', () => {
  it('control: asks the real list_notes edge for the current folder', () => {
    const call = invoked.mock.calls.find((one) => one[0] === 'list_notes');

    expect(call, 'readWhatWasGiven never reached the production list_notes edge').toBeDefined();
    expect(call?.[1]).toEqual({ catalogFolder: FOLDER });
  });

  it('shows the admitted global note and this agent own admitted note', () => {
    expect(build).toContain(EVERYWHERE.rule + ' — in use');
    expect(build).toContain(MINE.rule + ' — in use');
    expect(build).toContain(PROJECT.rule + ' — in use');
  });

  it('keeps a candidate and another agent private note off Build screen', () => {
    expect(build).not.toContain(CANDIDATE.rule);
    expect(build).not.toContain(ANOTHER_AGENTS.rule);
  });

  it('keeps an in-use note left out by the current length limit off Build screen', () => {
    expect(build).not.toContain(LEFT_OUT.rule);
  });

  it('keeps notes with a place and scope pairing that Rust excludes off Build screen', () => {
    expect(build).not.toContain(LIBRARY_PROJECT_SCOPE.rule);
    expect(build).not.toContain(PROJECT_GLOBAL.rule);
  });

  it('control: the same left-out row remains visible in the real Memory catalog', () => {
    expect(leftOutInCatalog).toContain(LEFT_OUT.rule);
  });
});
