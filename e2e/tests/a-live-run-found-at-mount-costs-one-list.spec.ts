import { afterAll, beforeAll, expect, it } from 'vitest';

import type { PastRunRow } from '../../src/sections/run/io';
import { closeEverything, openApp } from '../harness';

const PROJECT = '/Users/x/ledger-ui';
const WORKSPACE = { id: PROJECT, name: 'Ledger', folder: PROJECT };
const RUN: PastRunRow = {
  folder: '20260903-181200__0198a1f2-3b4c-7d5e-8f60-000000000027',
  when: '2026-09-03 18:12',
  title: 'Deep research',
  workflowFile: 'deep-research.json',
  state: 'running',
  steps: 3,
  costUsd: null,
  said: null,
};
const ALREADY_GOING =
  '"Deep research" was already going when this window opened, so the lines from before are not ' +
  'here. Stop reaches it.';

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

it('finds a live run from its list row without opening the full run', async () => {
  const copies = Array.from({ length: 4 }, () => ({ value: [RUN] }));
  const app = await openApp({
    replies: {
      list_workspaces: [{ value: [WORKSPACE] }],
      list_runs: copies,
    },
  });
  try {
    await app.page.getByText(ALREADY_GOING, { exact: false }).waitFor({
      state: 'attached',
      timeout: 4_000,
    });
    const historyCalls = (await app.calls()).filter(
      (call) => call.cmd === 'list_runs' || call.cmd === 'read_run',
    );
    const listsHere = historyCalls.filter(
      (call) => call.cmd === 'list_runs' && call.args['folder'] === PROJECT,
    );

    expect(listsHere, 'mounting this workspace asked for its run list more than once').toHaveLength(
      1,
    );
    expect(
      historyCalls.some((call) => call.cmd === 'read_run'),
      'mounting opened the full run and fetched its lines even though the list row already ' +
        'carried the title and workflow file needed by the live bar',
    ).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
