/* WF-07: rozstrzygnięty przez most Start nie pyta aktywnego widoku o adres.
 *
 * 2026-09 — podmieniony jest tylko kabel Tauri. Paczka przechodzi przez rzeczywiste
 * openChat, parser drutu, magazyn i Feed. Brak obsługi nowego wiersza ma paść w wykonaniu,
 * nie na imporcie, a odmowa musi stać w rozmowie człowieka (niezmienniki 19 i 29).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useWorkspaces } from '../../state/workspaces';
import { runFor } from '../../state/run';
import { Feed } from './feed/feed';
import { feedFor } from './feed/live';
import { openChat } from './io';
import { setAtOnce } from './limits/chosen';

const { invoked, pumps } = vi.hoisted(() => ({
  invoked: vi.fn<(command: string, args: Record<string, unknown>) => Promise<unknown>>(),
  pumps: [] as Array<{ onmessage: ((batch: unknown[]) => void) | null }>,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown[]) => void) | null = null;

    public constructor() {
      pumps.push(this);
    }
  },
}));

const A = { id: '/work/lead-start-a', folder: '/work/lead-start-a', name: 'Project A' };
const B = { id: '/work/lead-start-b', folder: '/work/lead-start-b', name: 'Project B' };
const REQUEST = '01800000-0000-7000-8000-000000000001:01800000-0000-7000-8000-000000000002';
const CONVERSATION = '01800000-0000-7000-8000-000000000003';

function request(): Record<string, unknown> {
  return {
    kind: 'runRequested',
    agent: 'Lead',
    text: 'Starting Ship it in Project A',
    requestId: REQUEST,
    conversationId: CONVERSATION,
    workspace: A.folder,
    title: 'Ship it',
    fileName: 'ship-it.json',
    steps: [{ id: 'a', name: 'Only in A', kind: 'agent', at: { x: 0, y: 0 }, weight: 'ordinary' }],
    links: [],
  };
}

function markup(terminal: string): string {
  return renderToStaticMarkup(
    <Feed
      view={feedFor(terminal).view}
      portRef={() => {}}
      onToggle={() => {}}
      onAnswer={() => {}}
      onJumpToNewest={() => {}}
    />,
  );
}

function acceptCalls(): Array<[string, Record<string, unknown>]> {
  return invoked.mock.calls.filter(([command]) => command === 'accept_lead_start');
}

beforeEach(() => {
  invoked.mockReset();
  pumps.length = 0;
  setAtOnce(4);
  useWorkspaces.setState({ all: [A, B], activeId: B.id, said: null });
  invoked.mockImplementation(async (command) => {
    if (command === 'list_workflows') {
      // Pułapka starej drogi: ten sam tytuł istnieje w aktualnie wybranym projekcie B.
      return [
        {
          path: 'ship-it.json',
          place: 'project',
          workflow: {
            name: 'Ship it',
            steps: [{ id: 'b', name: 'Only in B', kind: 'agent' }],
            links: [],
          },
        },
      ];
    }
    return undefined;
  });
});

describe('the lead hands an addressed start to Rust', () => {
  it('forwards only the request identity while another workspace is active and stays there across await', async () => {
    const terminal = 'lead-start-addressed';
    await openChat(A.folder, terminal);
    const conversation = pumps.at(-1);
    let finish: (() => void) | undefined;
    invoked.mockImplementation(async (command) => {
      if (command === 'accept_lead_start') {
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
      }
      return undefined;
    });

    conversation?.onmessage?.([request()]);
    try {
      await vi.waitFor(() => expect(acceptCalls()).toHaveLength(1));
      const sent = acceptCalls()[0]?.[1];
      expect(sent?.requestId).toBe(REQUEST);
      expect(sent?.howManyAtOnce).toBe(4);
      expect(sent).not.toHaveProperty('folder');
      expect(sent).not.toHaveProperty('fileName');
      expect(sent).not.toHaveProperty('task');
      expect(invoked.mock.calls.filter(([command]) => command === 'list_workflows')).toEqual([]);
      expect(invoked.mock.calls.filter(([command]) => command === 'run_workflow')).toEqual([]);
      conversation?.onmessage?.([request()]);
      expect(acceptCalls()).toHaveLength(1);

      useWorkspaces.setState({ activeId: A.id });
      useWorkspaces.setState({ activeId: B.id });
      const runChannel = sent?.lines as { onmessage: (batch: unknown[]) => void };
      runChannel.onmessage([
        { kind: 'note', agent: 'Builder', text: 'Only Project A ran.', body: [] },
      ]);
      expect(markup(A.folder)).toContain('Only Project A ran.');
      expect(markup(B.folder)).not.toContain('Only Project A ran.');
    } finally {
      finish?.();
    }
  });

  it('shows the precise prestart refusal in the originating conversation', async () => {
    const terminal = 'lead-start-refused';
    await openChat(A.folder, terminal);
    const refusal =
      'Nothing started: Ship it changed after the lead selected it. Ask to start it again.';
    invoked.mockImplementation(async (command) => {
      if (command === 'accept_lead_start') throw refusal;
      return undefined;
    });

    pumps.at(-1)?.onmessage?.([{ ...request(), requestId: REQUEST.replace(/2$/, '4') }]);
    await vi.waitFor(() => expect(markup(terminal)).toContain(refusal));
    expect(markup(B.folder)).not.toContain(refusal);
    expect(acceptCalls()).toHaveLength(1);
  });

  it('does not execute a run command from a prose suggestion', async () => {
    await openChat(A.folder, 'lead-start-prose');
    pumps.at(-1)?.onmessage?.([
      {
        kind: 'suggested',
        agent: 'Lead',
        text: 'You could run Ship it.',
        command: '/run ship-it',
        auto: false,
      },
    ]);
    await Promise.resolve();
    expect(acceptCalls()).toEqual([]);
    expect(invoked.mock.calls.filter(([command]) => command === 'run_workflow')).toEqual([]);
  });

  it('does not replace the live run display while Rust refuses an already busy workspace', async () => {
    const terminal = 'lead-start-busy';
    const session = runFor(A.folder);
    session.getState().nowRunning('Already working', [], A.folder, 'existing.json', []);
    await openChat(A.folder, terminal);
    const refusal = 'A run is already working in this folder. Stop it before starting another.';
    invoked.mockImplementation(async (command) => {
      if (command === 'accept_lead_start') throw refusal;
      return undefined;
    });
    pumps.at(-1)?.onmessage?.([{ ...request(), requestId: REQUEST.replace(/2$/, '5') }]);
    await vi.waitFor(() => expect(markup(terminal)).toContain(refusal));
    expect(session.getState().workflow).toBe('Already working');
    session.getState().nowRunning('', [], null, undefined, []);
  });
});
