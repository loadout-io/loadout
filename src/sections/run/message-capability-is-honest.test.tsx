/* WF-08 RED: transport → parser → Run/real Entry, włącznie z handlerem dostarczonym przez Run. */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { EntryProps } from './entry/entry';

const { seen, pumps, invoke } = vi.hoisted(() => ({
  seen: [] as EntryProps[],
  pumps: [] as Array<{ onmessage: ((batch: unknown[]) => void) | null }>,
  invoke: vi.fn<(name: string, args: Record<string, unknown>) => Promise<unknown>>(
    async () => undefined,
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    public onmessage: ((batch: unknown[]) => void) | null = null;
    public constructor() {
      pumps.push(this);
    }
  },
}));
vi.mock('./entry/entry', async (importOriginal) => {
  const real = await importOriginal<typeof import('./entry/entry')>();
  return {
    ...real,
    Entry: (props: EntryProps) => {
      seen.push(props);
      return real.Entry(props);
    },
  };
});

const { default: Run } = await import('./index');
const { openChat } = await import('./io');
const { runFor } = await import('../../state/run');
const { useWorkspaces } = await import('../../state/workspaces');
const FOLDER = '/work/message-capability';
const RUN = '01980000-0000-7000-8000-000000000001';

afterEach(() => {
  runFor(FOLDER).getState().nowRunning('', []);
  seen.length = 0;
  invoke.mockClear();
});

async function screen(): Promise<string> {
  useWorkspaces.setState({
    activeId: FOLDER,
    all: [{ id: FOLDER, name: 'Messages', folder: FOLDER }],
  });
  runFor(FOLDER)
    .getState()
    .nowRunning(
      'Work',
      [
        { id: 'builder', name: 'Builder', state: 'running' },
        { id: 'guide', name: 'Guide', state: 'running' },
      ],
      FOLDER,
      'work.json',
      [],
    );
  await openChat(FOLDER);
  pumps.at(-1)?.onmessage?.([
    {
      kind: 'stepSession',
      agent: 'Builder',
      runId: RUN,
      nodeKey: 'builder#2',
      canReceive: false,
      finished: false,
    },
    {
      kind: 'stepSession',
      agent: 'Guide',
      runId: RUN,
      nodeKey: 'guide#2',
      canReceive: true,
      finished: false,
    },
  ]);
  return renderToStaticMarkup(<Run />);
}

it('offers only real listeners, and an explicitly unsupported address never falls back to the Lead', async () => {
  invoke.mockImplementation(async (name) =>
    name === 'send_to_step'
      ? {
          runId: RUN,
          nodeKey: 'builder#2',
          result: 'unsupportedDuringRun',
          said: 'Builder does not accept messages while it is running.',
        }
      : undefined,
  );
  const markup = await screen();
  const entry = seen.at(-1);
  expect(entry).toBeDefined();
  expect(entry?.talkingTo).toEqual(['Guide']);
  expect(markup).toContain('Guide to reach it');
  const said = await entry?.onSayToAgent('@builder#2 explain the result', []);
  expect(said).toBe('Builder does not accept messages while it is running.');
  expect(invoke).toHaveBeenCalledWith('send_to_step', {
    folder: FOLDER,
    runId: RUN,
    nodeKey: 'builder#2',
    text: 'explain the result',
  });
  expect(invoke.mock.calls.some(([name]) => name === 'say_to_orchestrator')).toBe(false);
});

it('a delayed Entry action keeps its old run and loop attempt instead of addressing the next one', async () => {
  invoke.mockImplementation(async (name) =>
    name === 'send_to_step'
      ? {
          runId: RUN,
          nodeKey: 'guide#2',
          result: 'staleRun',
          said: 'That run is no longer active here. Read its current status before trying again.',
        }
      : undefined,
  );
  await screen();
  const oldEntry = seen.at(-1);
  pumps.at(-1)?.onmessage?.([
    {
      kind: 'stepSession',
      agent: 'Guide',
      runId: '01980000-0000-7000-8000-000000000002',
      nodeKey: 'guide#3',
      canReceive: true,
      finished: false,
    },
  ]);
  const said = await oldEntry?.onSayToAgent('@guide#2 continue carefully', []);
  expect(said).toBe(
    'That run is no longer active here. Read its current status before trying again.',
  );
  expect(invoke).toHaveBeenCalledWith('send_to_step', {
    folder: FOLDER,
    runId: RUN,
    nodeKey: 'guide#2',
    text: 'continue carefully',
  });
  expect(invoke.mock.calls.some(([name]) => name === 'say_to_orchestrator')).toBe(false);
});
