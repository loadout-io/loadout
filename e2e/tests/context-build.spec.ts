/* CT-04 w prawdziwej przeglądarce: klik przechodzi przez widok, magazyn i cztery krawędzie.
 * Odpowiedzi stoją na granicy Rusta; kontrakt procesu i publikację sądzi moduł integracyjny. */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const WORKFLOW_CONTEXT = resolve(ROOT, 'src-tauri/src/commands/workflow_context.rs');

const SET = {
  schema: 1,
  id: 'ct-build',
  title: 'Checkout',
  description: 'Keep the purchase rules together.',
  archived: false,
  draftRevision: 2,
  latestReadyRevision: null,
  createdAt: '2026-09-08T10:00:00Z',
  changedAt: '2026-09-08T10:00:00Z',
};
const READY_SET = { ...SET, latestReadyRevision: 'revision-1' };
const READ = {
  set: SET,
  draft: {
    schema: 1,
    sources: [
      {
        id: 'typed',
        kind: 'text',
        name: 'Typed material',
        description: '',
        text: 'The total stays visible while the cart changes.',
      },
    ],
    excluded: [],
    howToPrepare: '',
    requirements: [],
  },
  revision: 'draft-2',
};
const BUILD = {
  operationId: '11111111-1111-4111-8111-111111111111',
  setId: SET.id,
  generation: 7,
  draftRevision: 2,
  stage: 'extracting',
  end: 'running',
  app: 'claude-code',
  requestedModel: null,
  model: 'sonnet',
  batchesDone: 1,
  batchesTotal: 2,
  sources: [
    { sourceId: 'typed', part: 'fragment 1', outcome: 'processed', said: 'Processed.' },
    { sourceId: 'picture', part: 'whole', outcome: 'unknown', said: '' },
  ],
  said: 'Claude Code is reading batch 2 of 2.',
  revisionId: null,
  startedAt: '2026-09-08T10:00:00Z',
  changedAt: '2026-09-08T10:00:01Z',
};
const CANCELLED = {
  ...BUILD,
  end: 'cancelled',
  said: 'You stopped this build, and Loadout made sure the agent stopped.',
};
const FINDING = {
  id: 'finding-1',
  kind: 'requirement',
  text: 'Keep the total visible.',
  condition: 'while editing the cart',
  sources: [{ sourceId: 'typed', part: 'fragment 1' }],
  topic: 'checkout',
  conflictsWith: [],
  origin: 'generated',
};
const REVISION = {
  id: 'revision-1',
  setId: SET.id,
  draftRevision: 2,
  app: 'claude-code',
  requestedModel: null,
  model: 'sonnet',
  createdAt: '2026-09-08T10:01:00Z',
  origin: 'generated',
  topics: [{ id: 'checkout', title: 'Checkout' }],
  findings: [FINDING],
  questions: ['Which total wins?'],
  conflicts: [],
  sources: BUILD.sources,
};
const HUMAN_REVISION = {
  ...REVISION,
  id: 'revision-2',
  origin: 'human',
  findings: [
    FINDING,
    {
      ...FINDING,
      id: 'finding-human',
      text: 'Keep both totals when the sources disagree.',
      origin: 'human',
    },
  ],
};

function copies<T>(value: T, count: number): readonly TauriReply[] {
  return Array.from({ length: count }, () => ({ value }) as TauriReply);
}

const APPS = [
  { app: 'claude-code', state: 'found', version: '2.1.263' },
  { app: 'codex', state: 'found', version: '0.153.0' },
];

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('building a context beside its material', () => {
  it('shows the one route from an empty set through a build to a ready version', async () => {
    /* 2026-09-09 (UX-3) — the middle sentence must be the bytes that disable this set in the
     * workflow picker; a second test literal would let the two screens drift together unnoticed. */
    const rust = existsSync(WORKFLOW_CONTEXT) ? readFileSync(WORKFLOW_CONTEXT, 'utf8') : '';
    const catalog = /fn catalog_choice[\s\S]*?\n}\n\nfn inspect_pin/.exec(rust)?.[0] ?? '';
    const cannotAdd =
      /None => \(\s*None,\s*Vec::new\(\),\s*Some\("([^"]+)"\.to_owned\(\)\),\s*\)/.exec(
        catalog,
      )?.[1] ?? '';
    expect(cannotAdd).not.toBe('');

    const empty = await openApp({
      replies: {
        check_agent_apps: copies(APPS, 4),
        list_context_sets: copies([SET], 6),
        read_context_set: copies({ ...READ, draft: { ...READ.draft, sources: [] } }, 6),
        read_context_build: copies({ build: null, revision: null, buildWith: 'claude-code' }, 6),
      },
    });
    try {
      await empty.page.locator('[data-section-switch="context"]').click();
      await empty.page.locator('[data-context-set]').first().click();
      await empty.page.locator('[data-set-next]').waitFor({ state: 'visible' });
      expect(await empty.page.locator('[data-set-next]').innerText()).toBe(
        'Add material to this set before it can be built.',
      );
      expect(await empty.page.locator('[data-set-next]').count()).toBe(1);
    } finally {
      await empty.close();
    }

    const failed = await openApp({
      replies: {
        check_agent_apps: copies(APPS, 4),
        list_context_sets: copies([SET], 6),
        read_context_set: copies(READ, 6),
        read_context_build: copies(
          {
            build: {
              ...BUILD,
              end: 'failed',
              batchesDone: 0,
              batchesTotal: 1,
              sources: [],
              said: 'Claude Code could not build this context.',
            },
            revision: null,
            buildWith: 'claude-code',
          },
          6,
        ),
      },
    });
    try {
      await failed.page.locator('[data-section-switch="context"]').click();
      await failed.page.locator('[data-context-set]').first().click();
      await failed.page.locator('[data-set-next]').waitFor({ state: 'visible' });
      expect(await failed.page.locator('[data-set-next]').innerText()).toBe(cannotAdd);
      expect(await failed.page.locator('[data-tab="overview"]').count()).toBe(0);
      expect(await failed.page.locator('[data-set-next]').innerText()).toBe(cannotAdd);
      expect(await failed.page.locator('[data-set-next]').count()).toBe(1);
      expect(await failed.page.locator('[data-build-action]').innerText()).toBe(
        'Try building again',
      );
    } finally {
      await failed.close();
    }

    const ready = await openApp({
      replies: {
        check_agent_apps: copies(APPS, 4),
        list_context_sets: copies([READY_SET], 6),
        read_context_set: copies({ ...READ, set: READY_SET }, 6),
        read_context_build: copies(
          {
            build: { ...BUILD, end: 'ready', revisionId: REVISION.id },
            revision: REVISION,
            buildWith: 'claude-code',
          },
          6,
        ),
      },
    });
    try {
      await ready.page.locator('[data-section-switch="context"]').click();
      await ready.page.locator('[data-context-set]').first().click();
      await ready.page.locator('[data-set-next]').waitFor({ state: 'visible' });
      expect(await ready.page.locator('[data-set-next]').innerText()).toBe(
        'This context is built, so a workflow step can add it.',
      );
      expect(await ready.page.locator('[data-tab="overview"]').count()).toBe(0);
      expect(await ready.page.locator('[data-set-next]').count()).toBe(1);
      expect(await ready.page.locator('[data-build-action]').innerText()).toBe('Rebuild context');
    } finally {
      await ready.close();
    }
  }, 90_000);

  it('keeps disk-backed progress across sections and Stop reaches Rust', async () => {
    const app = await openApp({
      replies: {
        check_agent_apps: copies(APPS, 4),
        list_context_sets: copies([SET], 8),
        read_context_set: copies(READ, 8),
        read_context_build: [
          { value: { build: null, revision: null, buildWith: 'claude-code' } },
          ...copies({ build: BUILD, revision: null, buildWith: 'claude-code' }, 40),
        ],
        build_context: [{ deferred: 'context-build' }],
        stop_context_build: copies(CANCELLED, 2),
        save_context_revision: copies(
          { build: CANCELLED, revision: HUMAN_REVISION, buildWith: 'claude-code' },
          2,
        ),
      },
    });
    try {
      const page = app.page;
      await page.evaluate(() => {
        crypto.randomUUID = () => '11111111-1111-4111-8111-111111111111';
      });
      await page.locator('[data-section-switch="context"]').click();
      await page.locator('[data-context-set]').first().click();
      await page.locator('[data-build-action]').waitFor({ state: 'visible' });
      expect(await page.locator('[data-context-build-controls]').innerText()).toContain(
        'Claude Code',
      );
      await page.locator('[data-build-action]').click();
      await expect.poll(() => page.locator('[data-build-action]').innerText()).toBe('Stop');
      await page.locator('[data-build-details] > summary').click();
      await expect
        .poll(() => page.locator('[data-build-progress]').innerText())
        .toContain('1 of 2');

      await page.locator('[data-section-switch="knowledge"]').click();
      await page.locator('[data-section-switch="context"]').click();
      await page.locator('[data-build-details] > summary').click();
      await expect
        .poll(() => page.locator('[data-build-progress]').innerText())
        .toContain('1 of 2');

      await page.locator('[data-build-action]').click();
      await expect
        .poll(() => page.locator('[data-build-said]').innerText())
        .toContain('made sure the agent stopped');
      const stops = (await app.calls()).filter((call) => call.cmd === 'stop_context_build');
      expect(stops).toHaveLength(1);
      expect(stops[0]?.args['setId']).toBe(SET.id);
      expect(typeof stops[0]?.args['operationId']).toBe('string');
      await app.settle('context-build', {
        value: { build: CANCELLED, revision: null, buildWith: 'claude-code' },
      });
    } finally {
      await app.close();
    }
  }, 90_000);

  it('saves a correction through the fourth command and shows its human origin', async () => {
    const app = await openApp({
      replies: {
        check_agent_apps: copies(APPS, 4),
        list_context_sets: copies([READY_SET], 6),
        read_context_set: copies({ ...READ, set: READY_SET }, 6),
        read_context_build: copies(
          { build: { ...BUILD, end: 'ready' }, revision: REVISION, buildWith: 'claude-code' },
          6,
        ),
        build_context: copies(
          { build: { ...BUILD, end: 'ready' }, revision: REVISION, buildWith: 'claude-code' },
          2,
        ),
        stop_context_build: copies(CANCELLED, 2),
        save_context_revision: copies(
          {
            build: { ...BUILD, end: 'ready' },
            revision: HUMAN_REVISION,
            buildWith: 'claude-code',
          },
          2,
        ),
      },
    });
    try {
      const page = app.page;
      await page.locator('[data-section-switch="context"]').click();
      await page.locator('[data-context-set]').first().click();
      await page.locator('#context-correction').fill('Keep both totals when the sources disagree.');
      await page.locator('[data-save-correction]').click();
      await expect
        .poll(() => page.locator('[data-context-overview]').innerText())
        .toContain('Yours');
      const saves = (await app.calls()).filter((call) => call.cmd === 'save_context_revision');
      expect(saves).toHaveLength(1);
      expect(JSON.stringify(saves[0]?.args)).toContain('Keep both totals');
    } finally {
      await app.close();
    }
  }, 90_000);
});
