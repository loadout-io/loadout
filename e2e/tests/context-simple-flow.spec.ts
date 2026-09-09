/* 2026-09-09: właściciel nie mógł znaleźć drogi od wklejenia do gotowego kontekstu.
 * Klik ma zapisać TE bajty, a dopiero potem zlecić budowanie; sam napis nie jest dowodem. */
import { readFileSync } from 'node:fs';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const SET = {
  schema: 1,
  id: 'simple-context',
  title: 'Checkout',
  description: '',
  archived: false,
  draftRevision: 1,
  latestReadyRevision: null,
  createdAt: '2026-09-09T00:00:00Z',
  changedAt: '2026-09-09T00:00:00Z',
};
const DRAFT = { schema: 1, sources: [], excluded: [], howToPrepare: '', requirements: [] };
const READ = { set: SET, draft: DRAFT, revision: 'read-1' };
const TEXT = 'Keep the promo code until the person removes it.';
const SAVED = {
  set: { ...SET, draftRevision: 2 },
  revision: 'saved-2',
  draft: {
    ...DRAFT,
    sources: [{ id: 'typed', kind: 'text', name: 'Typed material', description: '', text: TEXT }],
  },
};
const APPS = [
  { app: 'claude-code', state: 'found', version: '2.1.266' },
  { app: 'codex', state: 'found', version: '0.153.4' },
];
const EMPTY_BUILD = { build: null, revision: null, buildWith: 'claude-code' };
const PDF_BYTES = readFileSync(
  new URL('../fixtures/context/three-pages.pdf', import.meta.url),
).toString('base64');
const PDF_SOURCE = {
  id: 'paper',
  kind: 'pdf',
  name: 'three-pages.pdf',
  description: '',
  text: '',
  preparation: { state: 'needs', pagesDone: 0 },
  file: {
    path: 'sources/paper/original.pdf',
    revision: 'file-1',
    mime: 'application/pdf',
    bytes: 1400,
    fingerprint: 'pdf-1',
    derived: 0,
    pages: 3,
  },
};
const PDF_READ = { ...READ, draft: { ...DRAFT, sources: [PDF_SOURCE] } };
const PDF_WHOLE = {
  kind: 'whole',
  mime: 'application/pdf',
  base64: PDF_BYTES,
  operationId: 'pdf-import',
  fingerprint: 'pdf-1',
};
const PDF_READY = {
  ...PDF_READ,
  revision: 'pages-saved',
  draft: { ...PDF_READ.draft, sources: [{ ...PDF_SOURCE, preparation: { state: 'ready' } }] },
};
const REVISION = {
  id: 'ready-1',
  setId: SET.id,
  draftRevision: 2,
  app: 'claude-code',
  requestedModel: null,
  model: 'sonnet',
  createdAt: SET.createdAt,
  origin: 'generated',
  topics: [{ id: 'checkout', title: 'Checkout rules' }],
  findings: [
    {
      id: 'promo',
      kind: 'requirement',
      text: TEXT,
      condition: '',
      sources: [{ sourceId: 'typed', part: 'fragment 1' }],
      topic: 'checkout',
      conflictsWith: [],
      origin: 'generated',
    },
  ],
  questions: [],
  conflicts: [],
  sources: [],
};
const READY_BUILD = {
  operationId: 'built',
  setId: SET.id,
  generation: 1,
  draftRevision: 2,
  stage: 'ready',
  end: 'ready',
  app: 'claude-code',
  model: 'sonnet',
  requestedModel: null,
  batchesDone: 1,
  batchesTotal: 1,
  sources: [],
  said: 'This context is ready.',
  revisionId: REVISION.id,
  startedAt: SET.createdAt,
  changedAt: SET.changedAt,
};
function copies(value: unknown, count = 10): TauriReply[] {
  return Array.from({ length: count }, () => ({ value }));
}
const BASE = {
  check_agent_apps: copies(APPS),
  list_context_sets: copies([SET]),
  read_context_set: copies(READ),
  read_context_build: copies(EMPTY_BUILD),
};

beforeAll(async () => {
  const app = await openApp();
  for (let attempt = 0; ; attempt += 1) {
    try {
      await app.page.addScriptTag({
        type: 'module',
        url: '/src/sections/context/pdf-preparation.ts',
      });
      break;
    } catch (error) {
      if (attempt >= 1 || !String(error).includes('Execution context was destroyed')) throw error;
      await app.page.waitForLoadState('domcontentloaded');
    }
  }
  await app.close();
}, 180_000);
afterAll(closeEverything, 30_000);

async function enter(replies: Record<string, readonly TauriReply[]> = {}) {
  const app = await openApp({ replies: { ...BASE, ...replies } });
  await app.page.locator('[data-section-switch="context"]').click();
  await app.page.locator('[data-context-set]').first().click();
  await app.page.locator('#context-material').waitFor({ state: 'visible' });
  return app;
}

/* 2026-09-09: przeglądarka i worker PDF potrzebują na wspólnym runnerze więcej niż
 * domyślne 5 s. Sufit dotyczy całego scenariusza; asercja przygotowania stron nadal
 * ma własne 30 s, a pozostałe asercje i ich wymagania pozostają takie same. */
describe('a context takes one build action', { timeout: 60_000 }, () => {
  it('offers Build beside the material with optional settings folded away', async () => {
    const app = await enter();
    try {
      expect(await app.page.locator('[data-build-action]').isVisible()).toBe(true);
      expect(await app.page.locator('#context-preparation').isVisible()).toBe(false);
      expect(await app.page.locator('#context-requirements').isVisible()).toBe(false);
      expect(await app.page.locator('#context-build-model').isVisible()).toBe(false);
      await app.page.locator('#context-material').fill(TEXT);
      expect(await app.page.locator('[data-build-action]').isEnabled()).toBe(true);
    } finally {
      await app.close();
    }
  });

  it('saves the current material once and waits for the saved revision before building', async () => {
    const app = await enter({
      save_context_draft: [{ deferred: 'save' }],
      build_context: [{ deferred: 'build' }],
    });
    try {
      await app.page.locator('#context-material').fill(TEXT);
      expect(await app.page.locator('[data-build-action]').isVisible()).toBe(true);
      await app.page.locator('[data-build-action]').click();
      await expect
        .poll(async () => (await app.calls()).filter((c) => c.cmd === 'save_context_draft').length)
        .toBe(1);
      expect((await app.calls()).filter((c) => c.cmd === 'build_context')).toHaveLength(0);
      expect(await app.page.locator('[data-build-action]').isDisabled()).toBe(true);
      await app.page
        .locator('[data-build-action]')
        .evaluate((button: HTMLButtonElement) => button.click());
      const saves = (await app.calls()).filter((c) => c.cmd === 'save_context_draft');
      expect(saves).toHaveLength(1);
      expect(saves[0]?.args['expectedRevision']).toBe('read-1');
      expect(JSON.stringify(saves[0]?.args)).toContain(TEXT);
      await app.settle('save', { value: SAVED });
      await expect
        .poll(async () => (await app.calls()).filter((c) => c.cmd === 'build_context').length)
        .toBe(1);
      expect(await app.page.locator('[data-build-action]').innerText()).toBe('Stop');
      // Odczyt postępu może jeszcze zwrócić poprzedni zapis. Stop ma przeżyć tę odpowiedź.
      await app.page.waitForTimeout(650);
      expect(await app.page.locator('[data-build-action]').innerText()).toBe('Stop');
    } finally {
      await app.close();
    }
  });

  it('keeps a save refusal visible and never starts an agent from the old material', async () => {
    const message = 'This context changed on disk. Open it again before saving.';
    const app = await enter({ save_context_draft: [{ error: message }] });
    try {
      await app.page.locator('#context-material').fill(TEXT);
      expect(await app.page.locator('[data-build-action]').isVisible()).toBe(true);
      await app.page.locator('[data-build-action]').click();
      await app.page.locator('[data-refusal]').waitFor({ state: 'visible' });
      expect(await app.page.locator('[data-refusal]').innerText()).toContain(message);
      expect((await app.calls()).filter((c) => c.cmd === 'build_context')).toHaveLength(0);
      expect(await app.page.locator('#context-material').inputValue()).toBe(TEXT);
      expect(await app.page.locator('[data-saved]').count()).toBe(0);
    } finally {
      await app.close();
    }
  });

  it('shows the prepared context without asking for another tab or discarding its sources', async () => {
    const ready = { ...SAVED, set: { ...SAVED.set, latestReadyRevision: REVISION.id } };
    const app = await enter({
      read_context_set: [{ value: READ }, ...copies(ready)],
      save_context_draft: [{ value: SAVED }],
      build_context: [
        { value: { build: READY_BUILD, revision: REVISION, buildWith: 'claude-code' } },
      ],
    });
    try {
      await app.page.locator('#context-material').fill(TEXT);
      await app.page.locator('[data-build-action]').click();
      await app.page.locator('[data-context-overview]').waitFor({ state: 'visible' });
      expect(await app.page.locator('[data-context-overview]').innerText()).toContain(
        'Checkout rules',
      );
      expect(await app.page.locator('[data-set-next]').innerText()).toContain(
        'workflow step can add it',
      );
      await app.page.locator('[data-edit-material]').click();
      expect(await app.page.locator('#context-material').inputValue()).toBe(TEXT);
      await app.page.locator('#context-material').fill(TEXT + ' Keep the saved discount too.');
      expect(await app.page.locator('[data-set-next]').innerText()).toContain('Build again');
    } finally {
      await app.close();
    }
  });

  it('prepares every PDF page before starting the agent with the same build click', async () => {
    const app = await enter({
      read_context_set: copies(PDF_READ),
      read_context_source: [{ value: PDF_WHOLE }],
      complete_context_source_preparation: [
        { value: PDF_READ },
        { value: PDF_READ },
        { value: PDF_READY },
      ],
      build_context: [{ deferred: 'build' }],
    });
    try {
      await app.page.locator('[data-build-action]').click();
      await expect
        .poll(async () => (await app.calls()).filter((c) => c.cmd === 'build_context').length, {
          timeout: 30_000,
        })
        .toBe(1);
      const calls = await app.calls();
      const pages = calls.filter((c) => c.cmd === 'complete_context_source_preparation');
      expect(pages.map((c) => (c.args['page'] as { number: number }).number)).toEqual([1, 2, 3]);
      expect(calls.findIndex((c) => c.cmd === 'build_context')).toBeGreaterThan(
        calls.findLastIndex((c) => c.cmd === 'complete_context_source_preparation'),
      );
    } finally {
      await app.close();
    }
  });

  it('stops preparation without starting an agent when the document arrives late', async () => {
    const app = await enter({
      read_context_set: copies(PDF_READ),
      read_context_source: [{ deferred: 'pdf' }],
    });
    try {
      await app.page.locator('[data-build-action]').click();
      await expect
        .poll(async () => (await app.calls()).filter((c) => c.cmd === 'read_context_source').length)
        .toBe(1);
      expect(await app.page.locator('[data-build-action]').innerText()).toBe('Stop');
      await app.page.locator('[data-build-action]').click();
      await app.settle('pdf', { value: PDF_WHOLE });
      await expect
        .poll(() => app.page.locator('[data-build-action]').innerText())
        .toBe('Build context');
      expect(
        (await app.calls()).filter((c) =>
          ['complete_context_source_preparation', 'build_context'].includes(c.cmd),
        ),
      ).toHaveLength(0);
      expect(await app.page.locator('[data-source-row="paper"]').innerText()).toContain(
        'three-pages.pdf',
      );
    } finally {
      await app.close();
    }
  });

  it('keeps the preparation failure visible and never asks the agent to work without that PDF', async () => {
    const said = 'This document could not be opened. Add another copy.';
    const app = await enter({
      read_context_set: copies(PDF_READ),
      read_context_source: [
        { value: { ...PDF_WHOLE, base64: Buffer.from('not a PDF').toString('base64') } },
      ],
      complete_context_source_preparation: [
        {
          value: {
            ...PDF_READ,
            draft: {
              ...DRAFT,
              sources: [{ ...PDF_SOURCE, preparation: { state: 'failed', said } }],
            },
          },
        },
      ],
    });
    try {
      await app.page.locator('[data-build-action]').click();
      await app.page.locator('[data-refusal]').waitFor({ state: 'visible' });
      expect(await app.page.locator('[data-refusal]').innerText()).toContain(said);
      expect((await app.calls()).filter((c) => c.cmd === 'build_context')).toHaveLength(0);
    } finally {
      await app.close();
    }
  });
});
