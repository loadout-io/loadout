import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, expect, it, vi } from 'vitest';

import { useWorkspaces } from '../../state/workspaces';
import type { Choice } from './choices';
import { launchRun } from './launch';
import { TabBar } from './tabs/tab-bar';
import { runTabs } from './tabs/store';

const { invoked, release } = vi.hoisted(() => {
  let finish: (() => void) | null = null;
  return {
    invoked: vi.fn(
      (): Promise<void> =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    ),
    release: (): void => {
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

const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

const DEEP_RESEARCH: Choice = {
  path: 'deep-research.json',
  name: 'Deep research',
  steps: [{ id: 'research', name: 'Research', state: 'pending' }],
};

const SHIP_IT: Choice = {
  path: 'ship-it.json',
  name: 'Ship it',
  steps: [{ id: 'ship', name: 'Ship', state: 'pending' }],
};

beforeEach(() => {
  invoked.mockClear();
  release();
  useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
  runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
});

it('leaves the tab named after the run that is going, not after the one that was turned down', async () => {
  const first = launchRun(DEEP_RESEARCH, 3);
  const refusal = await launchRun(SHIP_IT, 3);
  const state = runTabs.getState();
  const markup = renderToStaticMarkup(
    <TabBar
      tabs={state.tabs}
      activeId={state.activeId}
      busy={0}
      atOnce={3}
      waitingIn={null}
      onSelect={() => {}}
      onClose={() => {}}
      onOpenFolder={() => {}}
    />,
  );

  expect(
    refusal,
    'the second Run was turned down without a sentence for the person who pressed it',
  ).not.toBe(null);
  expect(
    markup,
    'the refused Run renamed the live card, so the tab now describes work that never started',
  ).toContain('Deep research');
  expect(markup).not.toContain('Ship it');

  release();
  await first;
});
