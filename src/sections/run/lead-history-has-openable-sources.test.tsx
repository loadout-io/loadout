/* WF-09/21: źródło statusu/historii przechodzi rzeczywisty kabel → parser → Feed. */
import { renderToStaticMarkup } from 'react-dom/server';
import type { ButtonHTMLAttributes, MouseEvent, ReactElement } from 'react';
import { expect, it, vi } from 'vitest';

import { Feed } from './feed/feed';
import { feedFor } from './feed/live';
import { openChat } from './io';
import { RunSourceControl } from './feed/run-source';
import { pastNow } from './past/store';
import { useWorkspaces } from '../../state/workspaces';

const { pumps, invoked } = vi.hoisted(() => ({
  pumps: [] as Array<{ onmessage: ((batch: unknown[]) => void) | null }>,
  invoked: vi.fn<(command: string, args: Record<string, unknown>) => Promise<unknown>>(
    async () => undefined,
  ),
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

it('shows the factual status and a control that opens its exact recorded source', async () => {
  const terminal = 'history-source-terminal';
  await openChat('/work/a', terminal);
  pumps.at(-1)?.onmessage?.([
    {
      kind: 'runSource',
      agent: 'Loadout',
      text: 'Parser fix stopped at Build: the checks failed.',
      workspace: '/work/a',
      runId: '01980000-0000-7000-8000-000000000001',
      runFolder: '20260904-100000__01980000-0000-7000-8000-000000000001',
      observedAt: '2026-09-05T10:00:00Z',
    },
  ]);
  const markup = renderToStaticMarkup(
    <Feed
      view={feedFor(terminal).view}
      portRef={() => {}}
      onToggle={() => {}}
      onAnswer={() => {}}
      onJumpToNewest={() => {}}
    />,
  );
  expect(markup).toContain('Parser fix stopped at Build: the checks failed.');
  expect(markup).toMatch(/<button[^>]*>[^<]*Open run source/);
  expect(markup).not.toContain('Stop run');
});

it('its real click handler reads the source workspace even when another card is selected', async () => {
  const terminal = 'history-source-click';
  const folder = '20260904-100000__01980000-0000-7000-8000-000000000001';
  invoked.mockImplementation(async (command) =>
    command === 'read_run'
      ? {
          folder,
          title: 'Parser fix',
          steps: [],
          handoffs: [],
          branches: [],
          state: 'failed',
        }
      : undefined,
  );
  await openChat('/work/a', terminal);
  pumps.at(-1)?.onmessage?.([
    {
      kind: 'runSource',
      agent: 'Loadout',
      text: 'Parser fix failed.',
      workspace: '/work/a',
      runId: '01980000-0000-7000-8000-000000000001',
      runFolder: folder,
      observedAt: '2026-09-05T10:00:00Z',
    },
  ]);
  const source = feedFor(terminal).view.history[0]?.source;
  expect(source).toBeDefined();
  if (source === undefined) throw new Error('The real feed discarded the source');
  useWorkspaces.setState({ activeId: '/work/b' });
  const control = RunSourceControl({ source }) as ReactElement<
    ButtonHTMLAttributes<HTMLButtonElement>
  >;
  control.props.onClick?.({} as MouseEvent<HTMLButtonElement>);
  await vi.waitFor(() =>
    expect(invoked).toHaveBeenCalledWith('read_run', { folder: '/work/a', run: folder }),
  );
  expect(pastNow().folder).toBe('/work/a');
  expect(pastNow().opened?.folder).toBe(folder);
  expect(
    invoked.mock.calls.some(([command]) => command === 'stop_run' || command === 'run_workflow'),
  ).toBe(false);
});
