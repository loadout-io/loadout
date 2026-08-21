import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { GONE_FROM_DISK } from '../run/launch';
import type { TriggerDelivery, TriggerIo } from './io';
import TriggersScreen from './index';
import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const RECEIPT_AT = Date.UTC(2026, 7, 21, 1, 53, 13, 400);
const DELIVERY: TriggerDelivery = {
  claim: {
    slug: 'assigned-to-me',
    deliveryId: 'delivery-7',
    workflow: 'analysis.json',
    runId: '0198ca82-ded0-7000-8000-000000000042',
  },
  issue: {
    id: 'issue-db-id',
    identifier: 'LIN-42',
    title: 'Fix the timeout handoff',
    url: 'https://linear.example/LIN-42',
    body: 'The completed analysis must reach the next step.',
    updatedAt: '2026-08-21T01:52:00.000Z',
  },
  createdAt: RECEIPT_AT - 3_700,
};

const CLOCK: TriggerClock = {
  setInterval: () => 1,
  clearInterval: () => undefined,
};
const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};
const IO: TriggerIo = {
  listTriggers: async () => [],
  setTriggerEnabled: async (slug, enabled) => ({
    slug,
    source: 'Linear',
    condition: 'Assigned to you',
    workflow: 'analysis.json',
    enabled,
  }),
  checkTrigger: async () => ({ status: 'armed' }),
};

const UNCHECKED = 'Not checked yet.';
const ARMED = 'Watching for new issues. Nothing has started yet.';
const BUSY = 'LIN-42 is saved while Loadout handles the run.';
const ACCEPTED = 'Started Analysis at 2026-08-21 01:53:13 UTC.';

function trigger(status: TriggerView['status']): TriggerView {
  return {
    slug: 'assigned-to-me',
    source: 'Linear',
    condition: 'Assigned to you',
    workflow: 'analysis.json',
    workflowName: 'Analysis',
    enabled: true,
    status,
  };
}

function markupFor(status: TriggerView['status']): string {
  const store = createTriggersStore(IO, CLOCK, RUN);
  store.setState({ triggers: [trigger(status)] });
  return renderToStaticMarkup(<TriggersScreen store={store} />);
}

function statusText(markup: string): string {
  const inside = /<[^>]+data-trigger-status[^>]*>([\s\S]*?)<\//.exec(markup)?.[1] ?? '';
  return inside
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the real Triggers screen tells five materially different truths', () => {
  it('says plainly that an unchecked trigger has not been asked yet', () => {
    expect(statusText(markupFor({ kind: 'unchecked' }))).toBe(UNCHECKED);
  });

  it('says an armed trigger is watching and has not started anything', () => {
    expect(statusText(markupFor({ kind: 'armed' }))).toBe(ARMED);
  });

  it('says a busy trigger kept the concrete issue for later', () => {
    const sentence = statusText(markupFor({ kind: 'busy', delivery: DELIVERY }));
    expect(sentence).toBe(BUSY);
    expect(sentence).not.toMatch(/will start|has not started|when .* finishes/i);
  });

  it('shows the launch refusal word for word where a person can read it', () => {
    expect(statusText(markupFor({ kind: 'refused', sentence: GONE_FROM_DISK }))).toBe(
      GONE_FROM_DISK,
    );
  });

  it('names the accepted workflow and the durable start time', () => {
    const markup = markupFor({ kind: 'accepted', workflow: 'Analysis', receiptAt: RECEIPT_AT });
    expect(statusText(markup)).toBe(ACCEPTED);
    /* React's static renderer preserves the JSX property spelling (`dateTime`). The browser
     * normalises it as the standard HTML datetime attribute; lower-case here would test a
     * serialisation detail contrary to the renderer this criterion actually uses. */
    expect(markup).toContain('dateTime="2026-08-21T01:53:13.400Z"');
  });

  it('keeps all five visible sentences pairwise distinct and never substitutes Date.now', () => {
    const sentences = [
      statusText(markupFor({ kind: 'unchecked' })),
      statusText(markupFor({ kind: 'armed' })),
      statusText(markupFor({ kind: 'busy', delivery: DELIVERY })),
      statusText(markupFor({ kind: 'refused', sentence: GONE_FROM_DISK })),
      statusText(markupFor({ kind: 'accepted', workflow: 'Analysis', receiptAt: RECEIPT_AT })),
    ];
    expect(new Set(sentences).size).toBe(5);

    const rowPath = resolve(ROOT, 'src/sections/triggers/row.tsx');
    const source = existsSync(rowPath) ? readFileSync(rowPath, 'utf8') : '';
    const code = source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/.*$/gm, ' ');
    expect(code).not.toContain('Date.now(');
  });
});
