/* CT-07: prawdziwy picker rozmowy wiąże dokładne wersje z backendowym podglądem Startu. */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-lead-context-e2e';
const WORKSPACE = { id: FOLDER, name: 'Lead context fixture', folder: FOLDER };
const AGENT = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000707',
  name: 'Lead fixture',
  summary: 'Plans with selected material',
  color: 'slate',
  instructions: 'Plan the requested work.',
  runsWith: 'claude-code',
  model: 'sonnet',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 10,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/lead.md',
};
const WORKFLOW = {
  path: 'ship.json',
  workflow: {
    format: 2,
    id: 'lead-context-workflow',
    name: 'Ship it',
    steps: [
      {
        kind: 'agent',
        id: 'build',
        name: 'Build',
        agent: AGENT.id,
        overrides: {},
        instructions: 'Build it.',
        folder: { use: 'project' },
        at: { x: 0, y: 0 },
      },
    ],
    links: [],
  },
};
const PIN = { id: 'store-rules', revision: 'revision-7', topics: ['checkout'] };
const CHOICE = {
  id: PIN.id,
  title: 'Store rules',
  description: 'Checkout constraints',
  revision: PIN.revision,
  topics: [{ id: 'checkout', title: 'Checkout' }],
  said: null,
};
const SELECTED = {
  id: PIN.id,
  title: CHOICE.title,
  revision: PIN.revision,
  selectedTopics: PIN.topics,
  topics: CHOICE.topics,
  source: 'workflow',
  update: null,
  said: null,
};
const EMPTY = {
  pins: {
    folder: FOLDER,
    sets: [],
    generation: 0,
    transcriptKeepsPreviousContext: false,
  },
  view: { catalog: [CHOICE], workflow: [], steps: [], warnings: [] },
};
const PINNED = {
  pins: {
    folder: FOLDER,
    sets: [PIN],
    generation: 1,
    transcriptKeepsPreviousContext: false,
  },
  view: { catalog: [CHOICE], workflow: [SELECTED], steps: [], warnings: [] },
};
const PINNED_AFTER_OLD_MESSAGES = {
  ...PINNED,
  pins: { ...PINNED.pins, transcriptKeepsPreviousContext: true },
};
const CHANGED = {
  pins: {
    folder: FOLDER,
    sets: [],
    generation: 2,
    transcriptKeepsPreviousContext: true,
  },
  view: { catalog: [CHOICE], workflow: [], steps: [], warnings: [] },
};
/* 2026-09-08 (CT-07) — PIĄTY ODCZYT, ZMIERZONY, NIE ZGADNIĘTY. Efekt czytający wybór biegnie
 * po KAŻDEJ zmianie rozmowy, a scenariusz zmienia ją cztery razy plus odczyt początkowy.
 * Czterowpisowa kolejka kończyła się przed ostatnim odczytem, a atrapa oddawała wtedy nic —
 * co przed CT-07 wjeżdżało do stanu jako „wybór, o którym nic nie wiadomo". Stan po ponownym
 * zaznaczeniu zestawu: przypięty znowu, nowe pokolenie, transcript nadal niesie stary materiał. */
const REPINNED_AFTER_CHANGE = {
  ...PINNED,
  pins: { ...PINNED.pins, generation: 3, transcriptKeepsPreviousContext: true },
};
const REQUEST = {
  kind: 'runRequested',
  agent: 'Lead',
  text: 'Starting Ship it',
  requestId: 'lead-context-request',
  conversationId: 'lead-context-conversation',
  workspace: FOLDER,
  title: 'Ship it',
  fileName: WORKFLOW.path,
  steps: [
    {
      id: 'build',
      name: 'Build',
      kind: 'agent',
      at: { x: 0, y: 0 },
      weight: 'ordinary',
    },
  ],
  links: [],
  context: [{ ...PIN, title: CHOICE.title }],
  contextGeneration: 1,
};
const APPEARS = 20_000;

function copies<T>(value: T, count = 24): readonly TauriReply[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_agents: copies([AGENT]),
    list_workflows: copies([WORKFLOW]),
    list_skills: copies([]),
    what_this_chat_pinned: [
      { value: EMPTY },
      { value: PINNED_AFTER_OLD_MESSAGES },
      { value: PINNED },
      { value: CHANGED },
      { value: REPINNED_AFTER_CHANGE },
    ],
    pin_context_to_chat: copies(null),
    accept_lead_start: [{ error: 'Context changed since this plan' }, { deferred: 'context-run' }],
  };
}

async function latestCall(app: RunningApp, command: string): Promise<TauriCall> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const calls = (await app.calls()).filter((call) => call.cmd === command);
    const found = calls.at(-1);
    if (found !== undefined) return found;
    await app.page.waitForTimeout(25);
  }
  throw new Error(`${command} was not called`);
}

async function send(app: RunningApp, call: TauriCall, message: unknown): Promise<void> {
  const channel = call.args['lines'];
  const match = typeof channel === 'string' ? /^__CHANNEL__:(\d+)$/.exec(channel) : null;
  expect(match, 'open_chat did not carry a real Tauri Channel').not.toBeNull();
  const id = match?.[1];
  if (id === undefined) throw new Error('the conversation Channel has no callback id');
  await app.page.evaluate(
    ({ slot, line }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const callback = host[slot];
      if (typeof callback !== 'function') throw new Error(`Channel ${slot} is not registered`);
      (callback as (payload: unknown) => void)({ index: 0, message: [line] });
    },
    { slot: `_${id}`, line: message },
  );
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('Lead Context', () => {
  it('keeps the conversation selection in the Start preview and sends its delivery choice', async () => {
    const app = await openApp({ replies: scene() });
    try {
      const picker = app.page.locator('[data-chat-context]');
      await picker.waitFor({ state: 'visible', timeout: APPEARS });
      const existingChat = await latestCall(app, 'open_chat');
      await send(app, existingChat, {
        kind: 'agent',
        agent: 'Lead',
        text: 'This answer used the earlier conversation selection.',
      });
      await picker.locator('summary').click();
      await picker.getByRole('checkbox', { name: CHOICE.title, exact: true }).click();
      await expect
        .poll(
          async () =>
            (await app.calls()).filter((call) => call.cmd === 'pin_context_to_chat').length,
        )
        .toBe(1);
      await expect
        .poll(() => app.page.locator('[data-chat-context-owner]').innerText())
        .toContain('Earlier messages still include the Context they were sent with.');

      await app.page.getByRole('button', { name: 'Start a new conversation' }).click();
      await expect
        .poll(
          async () =>
            (await app.calls()).filter((call) => call.cmd === 'pin_context_to_chat').length,
        )
        .toBe(2);
      const copied = (await app.calls())
        .filter((call) => call.cmd === 'pin_context_to_chat')
        .at(-1);
      expect(copied?.args['sets']).toEqual([PIN]);

      const chat = await latestCall(app, 'open_chat');
      await send(app, chat, REQUEST);
      const preview = app.page.locator('[data-lead-context-preview]');
      await preview.waitFor({ state: 'visible', timeout: APPEARS });
      await expect
        .poll(() => preview.innerText())
        .toContain(`${CHOICE.title} · version ${PIN.revision}`);
      await preview.getByRole('radio', { name: 'Give to selected steps' }).click();
      await preview.getByRole('button', { name: 'Start', exact: true }).click();
      await expect
        .poll(
          async () => (await app.calls()).filter((call) => call.cmd === 'accept_lead_start').length,
        )
        .toBe(1);
      await app.page
        .getByText('Context changed since this plan', { exact: true })
        .waitFor({ state: 'visible', timeout: APPEARS });

      const currentPicker = app.page.locator('[data-chat-context]');
      await currentPicker.locator('summary').click();
      await currentPicker.getByRole('checkbox', { name: CHOICE.title, exact: true }).click();
      await expect
        .poll(() => app.page.locator('[data-chat-context-owner]').innerText())
        .toContain('Earlier messages still include the Context they were sent with.');
      await expect.poll(() => preview.innerText()).toContain('Context changed since this plan');
      await preview.getByRole('button', { name: 'Keep previous selection and start' }).click();
      await expect
        .poll(
          async () => (await app.calls()).filter((call) => call.cmd === 'accept_lead_start').length,
        )
        .toBe(2);

      /* Kolejka atrapy krótsza od liczby odczytów robi z tego ekranu odmowę, a poprzednie
       * asercje przechodzą MIMO niej — szukają swojego zdania w tekście, który niesie oba.
       * To sprawdzenie trzyma fiksturę uczciwą. */
      await expect
        .poll(() => app.page.locator('[data-chat-context-owner]').innerText())
        .not.toContain('could not read this conversation Context');
      const accepted = await latestCall(app, 'accept_lead_start');
      expect(accepted.args['requestId']).toBe(REQUEST.requestId);
      expect(accepted.args['context']).toEqual({
        target: { place: 'steps', stepIds: ['build'] },
        keepPrevious: true,
      });
    } finally {
      await app.close();
    }
  }, 90_000);
});
