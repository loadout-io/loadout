import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';

import { createWorkspacesStore } from '../../../state/run-tabs';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import { createFeed } from '../feed/model';
import { atWork, roster } from '../rail/roster';
import { TabBar } from './tab-bar';

it('leaves no live dot or close question after every agent in the roster has finished', () => {
  const feed = createFeed(sealedScroller());
  feed.appendLines([
    line.read(1, 0, 'Forge', 'src/card.ts'),
    line.done(2, 10, 'Forge', 'Done', 'well'),
    line.read(3, 20, 'Needle', 'src/card.test.ts'),
    line.done(4, 30, 'Needle', "Didn't work", 'badly'),
  ]);
  const cards = roster({
    view: feed.view,
    agents: [
      { id: 'Forge', name: 'Forge', role: 'writes code', step: 'running' },
      { id: 'Needle', name: 'Needle', role: 'runs checks', step: 'running' },
    ],
  });
  const working = atWork(cards);
  const tab = {
    id: '/Users/x/ledger-ui',
    name: 'Ship it',
    path: '/Users/x/ledger-ui',
    agents: working,
  };
  const noop = () => undefined;
  const markup = renderToStaticMarkup(
    <TabBar
      tabs={[tab]}
      activeId={tab.id}
      busy={working}
      atOnce={3}
      waitingIn={null}
      onSelect={noop}
      onClose={noop}
      onOpenFolder={noop}
    />,
  );

  expect(cards.map((card) => card.status)).toEqual(['done', 'failed']);
  expect(working, 'finished and failed history rows were still counted as live agents').toBe(0);
  expect(markup, 'the tab still pulses after every agent has finished').not.toContain(
    'data-live-dot',
  );

  const tabs = createWorkspacesStore(() => Promise.resolve());
  tabs.getState().open(tab);
  tabs.getState().requestClose(tab.id);
  expect(tabs.getState().pendingClose, 'closing a quiet tab still asks about agents').toBeNull();
  expect(tabs.getState().tabs).toEqual([]);
});
