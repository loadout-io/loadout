/* T-132 AC-3: frozen receipt reaches history through the command line and a real row click.
 *
 * The only runtime mock is the existing browser harness' `window.__TAURI_INTERNALS__` boundary.
 * No store setter, history action or component handler is called directly. Future receipt fields
 * ride on objects that still satisfy today's `PastStep` shape, so before fails on missing visible
 * behaviour rather than TypeScript collection.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { Note } from '../../src/state/memory';
import type { PastRun, PastRunRow, PastStep } from '../../src/sections/run/io';
import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const PROJECT = '/Users/somebody/Projects/t132-memory-history';
const WORKSPACE = { id: PROJECT, name: 'T132 memory history', folder: PROJECT };
const FOLDER = '20260826-132000__019b0132-0000-7000-8000-000000000132';
const STEP_A = '019b0132-0000-7000-8000-00000000013a';
const STEP_B = '019b0132-0000-7000-8000-00000000013b';
const OPAQUE_ORIGIN = '019b0131-aaaa-7bbb-8ccc-0123456789ab';
const CURRENT_ONLY = 'CURRENT CATALOG MUST NOT REWRITE FINISHED HISTORY';
const CURRENT_ORIGIN = 'newer-project-that-was-not-there-during-the-run';

const FIELD = '[aria-label="Command line"]';
const APPEARS = 4_000;

const ROW: PastRunRow = {
  folder: FOLDER,
  when: '2026-08-26 13:20',
  title: 'Frozen memory history',
  state: 'failed',
  steps: 2,
  costUsd: null,
  said: null,
};

const BASE_A: PastStep = {
  id: STEP_A,
  tile: 'worker',
  name: 'Repeated worker',
  agent: 'Memory witness',
  state: 'succeeded',
  summary: '',
  error: '',
  costUsd: null,
  lines: [],
};

const BASE_B: PastStep = {
  ...BASE_A,
  id: STEP_B,
  tile: 'worker#2',
  state: 'failed',
  error: 'This copy failed after its agent started.',
};

// Variables are deliberate: structural typing accepts additive wire fields while a direct
// `PastStep` literal would reject the future key before the browser could run the criterion.
const STEP_WITH_IMPORTED_RECEIPT = {
  ...BASE_A,
  memory: [
    {
      reference: 'memory/notes/same.md',
      hash: '1111222233334444',
      bytes: 41,
      address: { place: 'library', id: 'same' },
      project: OPAQUE_ORIGIN,
      from: null,
      leftOut: false,
    },
  ],
};

const STEP_WITH_DEFERRED_RECEIPT = {
  ...BASE_B,
  memory: [
    {
      reference: '.loadout/memory/notes/same.md',
      hash: 'aaaabbbbccccdddd',
      bytes: 73,
      address: { place: 'project', id: 'same' },
      project: null,
      from: OPAQUE_ORIGIN,
      leftOut: true,
    },
  ],
};

const OPENED: PastRun = {
  folder: FOLDER,
  when: ROW.when,
  title: ROW.title,
  state: ROW.state,
  workflowFile: 'memory-history.json',
  steps: [STEP_WITH_IMPORTED_RECEIPT, STEP_WITH_DEFERRED_RECEIPT],
  handoffs: [],
  branches: [],
  said: null,
};

const CONTRADICTORY_CURRENT_NOTE: Note = {
  place: 'library',
  id: 'same',
  title: 'A newer version',
  rule: CURRENT_ONLY,
  because: 'This text exists only after the finished run.',
  status: 'in-use',
  scope: 'everywhere',
  length: 999,
  occurrences: 9,
  modified: '2026-08-26T14:00:00Z',
  agent: null,
  project: CURRENT_ORIGIN,
  from: null,
  leftOut: false,
};

function copies<T>(value: T, count = 8): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_workflows: copies([]),
    list_agents: copies([]),
    list_handoffs: copies([]),
    list_notes: copies([CONTRADICTORY_CURRENT_NOTE]),
    list_runs: copies([ROW]),
    read_run: copies(OPENED),
  };
}

async function waitForCalls(
  app: RunningApp,
  command: string,
  count: number,
): Promise<readonly TauriCall[]> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const calls = (await app.calls()).filter((call) => call.cmd === command);
    if (calls.length >= count) return calls;
    await app.page.waitForTimeout(20);
  }
  const calls = (await app.calls()).filter((call) => call.cmd === command);
  expect(
    calls.length,
    `${command} did not cross the real browser boundary ${String(count)} times`,
  ).toBeGreaterThanOrEqual(count);
  return calls;
}

async function openFinishedRun(): Promise<RunningApp> {
  const app = await openApp({ replies: scene() });

  // Load contradictory current memory through its real screen, then return to Run. This seeds
  // the production singleton without importing or calling its setter.
  await app.page.locator('[data-section-switch="knowledge"]').click();
  await app.page.locator('main[data-section="knowledge"]').waitFor({
    state: 'attached',
    timeout: APPEARS,
  });
  await app.page.getByText(CURRENT_ONLY, { exact: true }).waitFor({
    state: 'attached',
    timeout: APPEARS,
  });
  await app.page.locator('[data-section-switch="run"]').click();
  await app.page.locator(FIELD).waitFor({ state: 'attached', timeout: APPEARS });

  const listCallsBefore = (await app.calls()).filter((call) => call.cmd === 'list_runs').length;
  await app.page.fill(FIELD, '/history');
  await app.page.press(FIELD, 'Enter');
  const row = app.page.locator(`button[data-history-row="${FOLDER}"]`);
  await row.waitFor({ state: 'attached', timeout: APPEARS });

  const listCalls = await waitForCalls(app, 'list_runs', listCallsBefore + 1);
  expect(listCalls.at(-1)?.args).toEqual({ folder: PROJECT });

  const readsBefore = (await app.calls()).filter((call) => call.cmd === 'read_run').length;
  await row.click();
  await app.page.locator(`[data-past-run="${FOLDER}"]`).waitFor({
    state: 'attached',
    timeout: APPEARS,
  });
  const reads = await waitForCalls(app, 'read_run', readsBefore + 1);
  expect(reads.at(-1)?.args).toEqual({ folder: PROJECT, run: FOLDER });
  expect(reads.length - readsBefore, 'one row click must request exactly one run').toBe(1);
  return app;
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('frozen memory reaches real history controls', () => {
  it('opens /history from the rendered command line and addresses read_run from the clicked row', async () => {
    const app = await openFinishedRun();
    try {
      expect(await app.page.locator(`[data-past-step="${STEP_A}"]`).count()).toBe(1);
      expect(await app.page.locator(`[data-past-step="${STEP_B}"]`).count()).toBe(1);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('shows each delivered or deferred receipt only under its physical step UUID', async () => {
    const app = await openFinishedRun();
    try {
      const first = (await app.page.locator(`[data-past-step="${STEP_A}"]`).textContent()) ?? '';
      const second = (await app.page.locator(`[data-past-step="${STEP_B}"]`).textContent()) ?? '';
      const screen = (await app.page.locator(`[data-past-run="${FOLDER}"]`).textContent()) ?? '';

      for (const [name, section] of [
        ['first', first],
        ['second', second],
      ] as const) {
        expect(section, `the ${name} physical step has no visible frozen-memory section`).toContain(
          'What this step knew',
        );
      }

      expect(first).toContain('memory/notes/same.md');
      expect(first).toContain('41 bytes');
      expect(first).toContain('11112222');
      expect(first).toContain('Given to this step');
      expect(first).toContain('Imported from ' + OPAQUE_ORIGIN);
      expect(first).not.toContain('.loadout/memory/notes/same.md');
      expect(first).not.toContain('Suggested after run ' + OPAQUE_ORIGIN);

      expect(second).toContain('.loadout/memory/notes/same.md');
      expect(second).toContain('73 bytes');
      expect(second).toContain('aaaabbbb');
      expect(second).toContain("Left out because it exceeded this run's length limit.");
      expect(second).toContain('Suggested after run ' + OPAQUE_ORIGIN);
      expect(second).not.toContain('Given to this step');
      expect(second).not.toContain('Imported from ' + OPAQUE_ORIGIN);

      expect(screen).not.toContain('Imported from another project');
      expect(screen).not.toContain(CURRENT_ONLY);
      expect(screen).not.toContain(CURRENT_ORIGIN);
    } finally {
      await app.close();
    }
  }, 90_000);
});
