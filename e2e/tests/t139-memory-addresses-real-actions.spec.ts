/* T-139 AC-3: legacy stays in its own zone and every real click carries a frozen address.
 *
 * This spec imports only the browser harness. State changes arrive as real Tauri replies
 * after DOM clicks; no store setter, component prop or action helper can satisfy it.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const A = '/Users/somebody/Projects/t139-a';
const B = '/Users/somebody/Projects/t139-b';
const WORKSPACE_A = { id: A, name: 'T139 A', folder: A };
const WORKSPACE_B = { id: B, name: 'T139 B', folder: B };
const MEMORY = 'main[data-section="memory"]';
const CALL_LIMIT = 4_000;
const TEST_LIMIT = 20_000;

type Place = 'library' | 'project';
type Status = 'suggested' | 'in-use';

interface WireNote {
  readonly place: Place;
  readonly id: string;
  readonly title: string;
  readonly rule: string;
  readonly because: string;
  readonly status: Status;
  readonly scope: 'everywhere' | 'this-project' | 'this-agent';
  readonly agent: string | null;
  readonly from: string | null;
  readonly length: number;
  readonly occurrences: number;
  readonly modified: string;
}

function note(
  place: Place,
  id: string,
  rule: string,
  status: Status = 'suggested',
  scope: WireNote['scope'] = 'this-project',
  from: string | null = null,
): WireNote {
  return {
    place,
    id,
    title: id,
    rule,
    because: 'The T-139 browser fixture keeps the physical address observable.',
    status,
    scope,
    agent: null,
    from,
    length: rule.length,
    occurrences: 1,
    modified: '2026-08-26T10:00:00Z',
  };
}

const LEGACY = note(
  'library',
  'legacy',
  'T139 legacy belongs to an earlier project until Move finishes.',
  'suggested',
  'this-project',
  'Earlier Project',
);
const LIBRARY_DUPLICATE = note(
  'library',
  'same',
  'T139 LIBRARY DUPLICATE must stay untouched.',
  'suggested',
  'everywhere',
);
const PROJECT_DUPLICATE = note('project', 'same', 'T139 PROJECT DUPLICATE is addressed.');
const MOVED_LEGACY = note(
  'project',
  'legacy',
  'T139 legacy belongs to an earlier project until Move finishes.',
);

const CATALOG_A = [note('project', 'a-only', 'T139 stale A must never repaint B')];
const CATALOG_B = [LEGACY, LIBRARY_DUPLICATE, PROJECT_DUPLICATE];
const AFTER_MOVE = [LIBRARY_DUPLICATE, PROJECT_DUPLICATE, MOVED_LEGACY];
const AFTER_USE = [
  LIBRARY_DUPLICATE,
  { ...PROJECT_DUPLICATE, status: 'in-use' as const },
  MOVED_LEGACY,
];
const AFTER_STOP = AFTER_MOVE;
const AFTER_DISCARD = [LIBRARY_DUPLICATE, MOVED_LEGACY];

function copies<T>(value: T, count = 8): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function baselineScene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE_A]),
    list_notes: copies([]),
    list_handoffs: copies([]),
  };
}

function addressedScene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE_A, WORKSPACE_B]),
    list_notes: [{ deferred: 'catalog-a' }, { deferred: 'catalog-b' }],
    list_handoffs: copies([]),
    move_note_to_project: [{ value: AFTER_MOVE }],
    put_note_to_use: [{ value: AFTER_USE }],
    stop_using_note: [{ value: AFTER_STOP }],
    discard_note: [{ value: AFTER_DISCARD }],
  };
}

async function openMemory(
  replies: Readonly<Record<string, readonly TauriReply[]>>,
): Promise<RunningApp> {
  const app = await openApp({ replies });
  await app.page.locator('[data-section-switch="memory"]').click();
  await app.page.locator(MEMORY).waitFor({ state: 'attached', timeout: CALL_LIMIT });
  return app;
}

async function waitForCalls(app: RunningApp, command: string, count: number): Promise<TauriCall[]> {
  const deadline = Date.now() + CALL_LIMIT;
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

async function addresses(app: RunningApp): Promise<string[]> {
  const found = await app.page
    .locator('[data-note-address]')
    .evaluateAll((rows) => rows.map((row) => row.getAttribute('data-note-address') ?? ''));
  return found.sort();
}

async function expectAddresses(app: RunningApp, wanted: readonly string[]): Promise<void> {
  const expected = [...wanted].sort();
  const deadline = Date.now() + CALL_LIMIT;
  while (Date.now() < deadline) {
    if (JSON.stringify(await addresses(app)) === JSON.stringify(expected)) return;
    await app.page.waitForTimeout(20);
  }
  expect(
    await addresses(app),
    'visible address multiset differs from the returned catalog',
  ).toEqual(expected);
}

function row(app: RunningApp, address: string) {
  return app.page.locator(`[data-note-address="${address}"]`);
}

async function expectLibraryMarker(app: RunningApp): Promise<void> {
  expect(await row(app, 'library:same').textContent()).toContain('T139 LIBRARY DUPLICATE');
}

async function settleBothCatalogs(app: RunningApp): Promise<void> {
  await waitForCalls(app, 'list_notes', 1);
  await app.page.locator('[data-workspace-open]').click();
  await app.page.locator(`[data-workspace-pick="${B}"]`).click();
  await waitForCalls(app, 'list_notes', 2);
  await app.settle('catalog-b', { value: CATALOG_B });
  await expectAddresses(app, ['library:legacy', 'library:same', 'project:same']);
  await app.settle('catalog-a', { value: CATALOG_A });
  await expectAddresses(app, ['library:legacy', 'library:same', 'project:same']);
  expect(await app.page.locator('[data-workspace-open]').textContent()).toContain('T139 B');
}

async function moveLegacyFromItsVisibleZone(app: RunningApp): Promise<void> {
  const earlier = app.page.locator('[data-zone="earlier-project"]');
  const legacy = earlier.locator('[data-note-address="library:legacy"]');
  expect(await legacy.count(), 'legacy is not a child of the earlier-project zone').toBe(1);
  expect(
    await app.page.locator('[data-zone="suggested"] [data-note-address="library:legacy"]').count(),
    'legacy leaked into Suggested before Move',
  ).toBe(0);
  expect(await legacy.textContent()).toMatch(/earlier project/i);
  expect(await legacy.getByRole('button').allInnerTexts()).toEqual(['Move to this project']);

  await legacy.getByRole('button', { name: 'Move to this project', exact: true }).click();
  await waitForCalls(app, 'move_note_to_project', 1);
  await expectAddresses(app, ['library:same', 'project:legacy', 'project:same']);
  expect(
    await app.page
      .locator('[data-zone="earlier-project"] [data-note-address="library:legacy"]')
      .count(),
  ).toBe(0);
  expect(
    await app.page.locator('[data-zone="suggested"] [data-note-address="project:legacy"]').count(),
  ).toBe(1);
  await expectLibraryMarker(app);
}

async function clickProjectDuplicateActions(app: RunningApp): Promise<void> {
  await row(app, 'project:same').getByRole('button', { name: 'Use this', exact: true }).click();
  await waitForCalls(app, 'put_note_to_use', 1);
  await expectAddresses(app, ['library:same', 'project:legacy', 'project:same']);
  await expectLibraryMarker(app);

  await row(app, 'project:same').getByRole('button', { name: 'Stop using', exact: true }).click();
  await waitForCalls(app, 'stop_using_note', 1);
  await expectAddresses(app, ['library:same', 'project:legacy', 'project:same']);
  await expectLibraryMarker(app);

  await row(app, 'project:same').getByRole('button', { name: 'Discard', exact: true }).click();
  await waitForCalls(app, 'discard_note', 1);
  await expectAddresses(app, ['library:same', 'project:legacy']);
  await expectLibraryMarker(app);
}

async function expectExactTape(app: RunningApp): Promise<void> {
  const commands = [
    'list_notes',
    'move_note_to_project',
    'put_note_to_use',
    'stop_using_note',
    'discard_note',
  ];
  const relevant = (await app.calls()).filter((call) => commands.includes(call.cmd));
  expect(
    relevant.map((call) => call.cmd),
    'the browser reordered or added a mutation',
  ).toEqual([
    'list_notes',
    'list_notes',
    'move_note_to_project',
    'put_note_to_use',
    'stop_using_note',
    'discard_note',
  ]);
  expect(relevant.map((call) => call.args)).toEqual([
    { catalogFolder: A },
    { catalogFolder: B },
    { catalogFolder: B, place: 'library', id: 'legacy' },
    { catalogFolder: B, place: 'project', id: 'same' },
    { catalogFolder: B, place: 'project', id: 'same' },
    { catalogFolder: B, place: 'project', id: 'same' },
  ]);
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('Memory keeps two roots and four human actions observable', () => {
  it(
    'mounts the real Memory screen and reaches its read command',
    async () => {
      const app = await openMemory(baselineScene());
      try {
        expect(await app.page.locator(`${MEMORY} h1`).textContent()).toBe('Memory');
        expect((await waitForCalls(app, 'list_notes', 1)).length).toBe(1);
      } finally {
        await app.close();
      }
    },
    TEST_LIMIT,
  );

  it(
    'ignores late A, locates legacy, then clicks Move, Use, Stop and Discard',
    async () => {
      const app = await openMemory(addressedScene());
      try {
        await settleBothCatalogs(app);
        await moveLegacyFromItsVisibleZone(app);
        await clickProjectDuplicateActions(app);
        await expectExactTape(app);
      } finally {
        await app.close();
      }
    },
    TEST_LIMIT,
  );
});
